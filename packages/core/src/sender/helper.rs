//! A playback subprocess: readiness, bounded diagnostics, liveness and teardown
//! belong to one lifetime. Spawning a process never proves playback is ready.

use super::SenderError;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

const READY: &str = "ON_AIR_READY";
const START_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct PlaybackHelper {
    child: Child,
    diagnostics: Arc<Mutex<VecDeque<String>>>,
    readers: Vec<tokio::task::JoinHandle<()>>,
}

impl PlaybackHelper {
    pub async fn start(command: std::process::Command) -> Result<Self, SenderError> {
        Self::start_with_timeout(command, START_TIMEOUT).await
    }

    async fn start_with_timeout(
        command: std::process::Command,
        timeout: Duration,
    ) -> Result<Self, SenderError> {
        let mut command = Command::from(command);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|e| SenderError::transport(format!("start AirPlay helper: {e}")))?;
        let diagnostics = Arc::new(Mutex::new(VecDeque::new()));
        let (ready_tx, ready_rx) = oneshot::channel();
        let stdout = child.stdout.take().expect("piped helper stdout");
        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut ready_tx = Some(ready_tx);
            while let Ok(Some(line)) = lines.next_line().await {
                if line == READY {
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(());
                    }
                } else {
                    eprintln!("AirPlay: {line}");
                }
            }
        });
        let stderr = child.stderr.take().expect("piped helper stderr");
        let errors = diagnostics.clone();
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("AirPlay: {line}");
                let mut errors = errors.lock();
                if errors.len() == 16 {
                    errors.pop_front();
                }
                errors.push_back(line.chars().take(512).collect());
            }
        });
        let mut helper = Self {
            child,
            diagnostics,
            readers: vec![stdout_task, stderr_task],
        };
        let outcome = tokio::select! {
            ready = ready_rx => ready.map_err(|_| "helper exited before playback was ready".to_string()),
            _ = tokio::time::sleep(timeout) => Err("receiver did not finish AirPlay setup within 30 seconds".to_string()),
        };
        match outcome {
            Ok(()) => {
                helper.check_health()?;
                Ok(helper)
            }
            Err(reason) => {
                helper.stop().await?;
                Err(helper.failure(&reason))
            }
        }
    }

    fn failure(&self, reason: &str) -> SenderError {
        let errors = self.diagnostics.lock();
        let detail = errors.back().map(String::as_str).unwrap_or(reason);
        SenderError::transport(format!("AirPlay playback failed: {detail}"))
    }

    pub fn check_health(&mut self) -> Result<(), SenderError> {
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(self.failure(&format!("helper exited: {status}"))),
            Err(error) => Err(SenderError::internal(format!(
                "check AirPlay helper: {error}"
            ))),
        }
    }

    pub async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.check_health()?;
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| SenderError::not_ready("AirPlay volume control is unavailable"))?;
        stdin
            .write_all(format!("{}\n", volume.min(100)).as_bytes())
            .await
            .map_err(|e| SenderError::transport(format!("set AirPlay volume: {e}")))
    }

    pub async fn stop(&mut self) -> Result<(), SenderError> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = stdin.write_all(b"STOP\n").await;
        }
        // Let pyatv cancel its stream and send TEARDOWN before another
        // receiver takes over. Hung helpers still have a bounded lifetime.
        match tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await {
            Ok(status) => {
                status.map_err(|e| SenderError::internal(format!("reap AirPlay helper: {e}")))?;
            }
            Err(_) => {
                self.kill_group();
                let _ = self.child.start_kill();
                self.child
                    .wait()
                    .await
                    .map_err(|e| SenderError::internal(format!("reap AirPlay helper: {e}")))?;
            }
        }
        for task in self.readers.drain(..) {
            let _ = task.await;
        }
        Ok(())
    }

    fn kill_group(&self) {
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            // This child created its own process group before exec. Its
            // descendants (uv/python) share it and must not outlive playback.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

impl Drop for PlaybackHelper {
    fn drop(&mut self) {
        self.kill_group();
        for task in &self.readers {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python(script: &str) -> std::process::Command {
        let mut command = std::process::Command::new("python3");
        command.args(["-u", "-c", script]);
        command
    }

    #[tokio::test]
    async fn the_actual_airplay_helper_reports_ready_and_accepts_volume() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut command = std::process::Command::new("python3");
        command
            .arg(manifest.join("src/sender/airplay_play.py"))
            .args([
                "--host",
                "127.0.0.1",
                "--url",
                "http://127.0.0.1/audio.wav",
                "--volume",
                "22",
            ])
            .env("PYTHONPATH", manifest.join("tests/fixtures/pyatv"))
            .env("PYTHONDONTWRITEBYTECODE", "1");
        let mut helper = PlaybackHelper::start(command).await.unwrap();
        helper.set_volume(37).await.unwrap();
        helper.check_health().unwrap();
        helper.stop().await.unwrap();
        assert!(
            helper.child.try_wait().unwrap().unwrap().success(),
            "the actual helper must stop gracefully"
        );
    }

    #[tokio::test]
    async fn spawn_without_readiness_is_a_failure_and_keeps_the_diagnostic() {
        let result =
            PlaybackHelper::start(python("raise SystemExit('receiver SETUP failed')")).await;
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("receiver SETUP failed"));
    }

    #[tokio::test]
    async fn an_unready_helper_is_timed_out_and_reaped() {
        let result = PlaybackHelper::start_with_timeout(
            python("import time; time.sleep(60)"),
            Duration::from_millis(100),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn readiness_volume_late_failure_and_stop_share_one_lifetime() {
        let mut helper = PlaybackHelper::start(python("import sys; print('ON_AIR_READY'); assert input() == '37'; raise SystemExit('receiver disconnected')")).await.unwrap();
        helper.set_volume(37).await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Err(error) = helper.check_health() {
                    break error;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(error.to_string().contains("receiver disconnected"));
        helper.stop().await.unwrap();
    }
}
