use crate::sender::airplay_mdns::CatalogDevice;
use crate::sender::{AudioSender, SenderError};
use reqwest::{Client, Url};
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_CATALOG_DEVICES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveBackend {
    OwnTone,
    Pyatv,
}

#[derive(Clone)]
enum PyatvLauncher {
    Python3,
    Uv(String),
}

/// Environment variable naming the `uv` binary to run pyatv with when
/// `python3` has no pyatv module. Falls back to `uv` on `PATH`.
pub const UV_ENV: &str = "ON_AIR_UV";

static PYATV_LAUNCHER: OnceLock<Option<PyatvLauncher>> = OnceLock::new();

/// OwnTone sidecar when present; otherwise pyatv RAOP to the receiver's IP.
pub struct AirPlaySender {
    name: String,
    output_id: String,
    base_url: String,
    stream_url: String,
    host: String,
    http: Client,
    child: Option<Child>,
    active_backend: Option<ActiveBackend>,
    volume: u8,
}

impl AirPlaySender {
    pub fn new(
        name: impl Into<String>,
        output_id: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        AirPlaySender {
            name: name.into(),
            output_id: output_id.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            stream_url: String::new(),
            host: String::new(),
            http: crate::sender::sonos::soap::http_client(),
            child: None,
            active_backend: None,
            volume: 50,
        }
    }

    pub fn with_radio(mut self, stream_url: impl Into<String>, host: impl Into<String>) -> Self {
        self.stream_url = stream_url.into();
        self.host = host.into();
        self
    }

    async fn put_json(&self, path: &str, body: serde_json::Value) -> Result<(), SenderError> {
        self.put_json_url(endpoint_url(&self.base_url, path)?, body)
            .await
    }

    async fn put_json_url(&self, url: Url, body: serde_json::Value) -> Result<(), SenderError> {
        let path = url.path().to_string();
        let response = self
            .http
            .put(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SenderError::transport(format!("OwnTone {path} failed: {e}")))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(SenderError::transport(format!(
                "OwnTone {} failed: {}",
                path,
                response.status()
            )))
        }
    }

    async fn owntone_reachable(&self) -> bool {
        let Ok(url) = endpoint_url(&self.base_url, "/api/outputs") else {
            return false;
        };
        let probe = crate::net::lan_http_client(std::time::Duration::from_millis(400));
        probe
            .get(url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn select_owntone_output(&self, selected: bool) -> Result<(), SenderError> {
        let url = output_url(&self.base_url, &self.output_id)?;
        self.put_json_url(url, serde_json::json!({ "selected": selected }))
            .await
    }

    async fn add_owntone_stream(&self) -> Result<(), SenderError> {
        let mut url = endpoint_url(&self.base_url, "/api/queue/items/add")?;
        url.query_pairs_mut().append_pair("uris", &self.stream_url);
        let response = self
            .http
            .post(url)
            .send()
            .await
            .map_err(|e| SenderError::transport(format!("OwnTone queue add failed: {e}")))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(SenderError::transport(format!(
                "OwnTone queue add failed: {}",
                response.status()
            )))
        }
    }

    async fn rollback_owntone_start(&self, player_may_be_running: bool) -> Option<SenderError> {
        let mut failures = Vec::new();
        if player_may_be_running {
            if let Err(error) = self
                .put_json("/api/player/stop", serde_json::json!({}))
                .await
            {
                failures.push(error.to_string());
            }
        }
        if let Err(error) = self.select_owntone_output(false).await {
            failures.push(error.to_string());
        }
        if failures.is_empty() {
            None
        } else {
            Some(SenderError::transport(failures.join("; ")))
        }
    }

    async fn start_owntone(&self) -> Result<(), SenderError> {
        self.select_owntone_output(true).await?;
        if !self.stream_url.is_empty() {
            if let Err(error) = self.add_owntone_stream().await {
                return match self.rollback_owntone_start(false).await {
                    Some(rollback) => Err(SenderError::RollbackFailed {
                        error: Box::new(error),
                        rollback: format!("OwnTone rollback also failed: {rollback}"),
                    }),
                    None => Err(error),
                };
            }
        }
        if let Err(error) = self
            .put_json("/api/player/play", serde_json::json!({}))
            .await
        {
            return match self.rollback_owntone_start(true).await {
                Some(rollback) => Err(SenderError::RollbackFailed {
                    error: Box::new(error),
                    rollback: format!("OwnTone rollback also failed: {rollback}"),
                }),
                None => Err(error),
            };
        }
        Ok(())
    }

    async fn start_pyatv(&mut self) -> Result<(), SenderError> {
        if self.host.is_empty() || self.stream_url.is_empty() {
            return Err(SenderError::transport(format!(
                "OwnTone is not running at {} (Linux AirPlay sidecar)",
                self.base_url
            )));
        }
        let host = self.host.clone();
        let stream_url = self.stream_url.clone();
        let volume = self.volume;
        let child = tokio::task::spawn_blocking(move || spawn_pyatv(&host, &stream_url, volume))
            .await
            .map_err(|e| SenderError::internal(format!("start AirPlay helper task: {e}")))??;
        self.child = Some(child);
        Ok(())
    }

    async fn stop_child(&mut self) -> Result<(), SenderError> {
        if let Some(mut child) = self.child.take() {
            tokio::task::spawn_blocking(move || terminate_child(&mut child))
                .await
                .map_err(|e| SenderError::internal(format!("stop AirPlay helper task: {e}")))??;
        }
        Ok(())
    }
}

impl Drop for AirPlaySender {
    fn drop(&mut self) {
        // A process outliving the sender keeps the radio stream and helper
        // runtime alive forever. Normal shutdown uses the async stop path
        // (which waits); this is the panic/cancellation safety net and must
        // not block whatever thread drops the sender, so it only kills.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}

fn endpoint_url(base: &str, path: &str) -> Result<Url, SenderError> {
    Url::parse(&format!("{}{}", base.trim_end_matches('/'), path))
        .map_err(|e| SenderError::internal(format!("invalid OwnTone URL: {e}")))
}

fn output_url(base: &str, output_id: &str) -> Result<Url, SenderError> {
    let mut url = endpoint_url(base, "/api/outputs/")?;
    url.path_segments_mut()
        .map_err(|_| SenderError::internal("invalid OwnTone base URL"))?
        .pop_if_empty()
        .push(output_id);
    Ok(url)
}

fn pyatv_command() -> Result<Command, SenderError> {
    match PYATV_LAUNCHER.get_or_init(detect_pyatv_launcher) {
        Some(PyatvLauncher::Python3) => Ok(Command::new("python3")),
        Some(PyatvLauncher::Uv(uv)) => {
            let mut cmd = Command::new(uv);
            cmd.args([
                "run",
                "--offline",
                "--with",
                "pyatv",
                "--python",
                "python3",
                "python3",
            ]);
            Ok(cmd)
        }
        None => Err(SenderError::transport(
            "AirPlay needs OwnTone on :3689 or a locally installed pyatv (`pip install pyatv`)",
        )),
    }
}

/// `$ON_AIR_UV` first, then `uv` from `PATH`.
fn uv_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os(UV_ENV) {
        let configured = configured.to_string_lossy().trim().to_string();
        if !configured.is_empty() {
            candidates.push(configured);
        }
    }
    candidates.push("uv".to_string());
    candidates
}

fn detect_pyatv_launcher() -> Option<PyatvLauncher> {
    if python_has_pyatv("python3") {
        return Some(PyatvLauncher::Python3);
    }
    uv_candidates()
        .into_iter()
        .find(|uv| uv_has_cached_pyatv(uv))
        .map(PyatvLauncher::Uv)
}

fn python_has_pyatv(bin: &str) -> bool {
    Command::new(bin)
        .args(["-c", "import pyatv"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn uv_has_cached_pyatv(bin: &str) -> bool {
    Command::new(bin)
        .args([
            "run",
            "--offline",
            "--with",
            "pyatv",
            "--python",
            "python3",
            "python3",
            "-c",
            "import pyatv",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn spawn_pyatv(host: &str, stream_url: &str, volume: u8) -> Result<Child, SenderError> {
    let mut cmd = pyatv_command()?;
    cmd.arg("-c")
        .arg(include_str!("airplay_play.py"))
        .arg("--host")
        .arg(host)
        .arg("--url")
        .arg(stream_url)
        .arg("--volume")
        .arg(volume.min(100).to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| SenderError::transport(format!("start AirPlay helper: {e}")))
}

fn write_pyatv_volume(writer: &mut impl Write, volume: u8) -> Result<(), SenderError> {
    writeln!(writer, "{}", volume.min(100))
        .and_then(|_| writer.flush())
        .map_err(|e| SenderError::transport(format!("set AirPlay volume: {e}")))
}

fn set_pyatv_volume(child: &mut Child, volume: u8) -> Result<(), SenderError> {
    if let Some(status) = child
        .try_wait()
        .map_err(|e| SenderError::internal(format!("check AirPlay helper: {e}")))?
    {
        return Err(SenderError::transport(format!(
            "AirPlay helper exited before volume change: {status}"
        )));
    }
    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| SenderError::not_ready("AirPlay helper volume control is unavailable"))?;
    write_pyatv_volume(stdin, volume)
}

fn terminate_child(child: &mut Child) -> Result<(), SenderError> {
    #[cfg(unix)]
    {
        let pid = child.id();
        let _ = Command::new("kill")
            .args(["-TERM", "--", &format!("-{pid}")])
            .status();
    }
    let _ = child.kill();
    child
        .wait()
        .map(|_| ())
        .map_err(|e| SenderError::internal(format!("reap AirPlay helper: {e}")))
}

#[async_trait::async_trait]
impl AudioSender for AirPlaySender {
    async fn start(&mut self) -> Result<(), SenderError> {
        if self.active_backend.is_some() {
            return Ok(());
        }
        if self.owntone_reachable().await {
            self.start_owntone().await?;
            self.active_backend = Some(ActiveBackend::OwnTone);
        } else {
            self.start_pyatv().await?;
            self.active_backend = Some(ActiveBackend::Pyatv);
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        match self.active_backend {
            Some(ActiveBackend::Pyatv) => {
                self.stop_child().await?;
            }
            Some(ActiveBackend::OwnTone) => {
                let mut failures = Vec::new();
                if let Err(error) = self
                    .put_json("/api/player/stop", serde_json::json!({}))
                    .await
                {
                    failures.push(error.to_string());
                }
                if let Err(error) = self.select_owntone_output(false).await {
                    failures.push(error.to_string());
                }
                if !failures.is_empty() {
                    return Err(SenderError::transport(failures.join("; ")));
                }
            }
            None => return Ok(()),
        }
        self.active_backend = None;
        Ok(())
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        let volume = volume.min(100);
        match self.active_backend {
            Some(ActiveBackend::OwnTone) => {
                self.put_json(
                    "/api/player/volume",
                    serde_json::json!({ "volume": volume }),
                )
                .await?;
            }
            Some(ActiveBackend::Pyatv) => {
                let child = self
                    .child
                    .as_mut()
                    .ok_or_else(|| SenderError::not_ready("AirPlay helper is not running"))?;
                set_pyatv_volume(child, volume)?;
            }
            None => return Err(SenderError::not_ready("AirPlay output is not active")),
        }
        self.volume = volume;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn transport(&self) -> &'static str {
        "airplay"
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pyatv_volume_command_is_clamped_and_flushed() {
        let mut command = Vec::new();
        super::write_pyatv_volume(&mut command, 101).unwrap();
        assert_eq!(command, b"100\n");
    }
}

pub fn platform_mode() -> &'static str {
    if cfg!(target_os = "macos") {
        "avroute-picker"
    } else {
        "owntone"
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OwnToneOutputs {
    #[serde(default)]
    outputs: Vec<OwnToneOutput>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OwnToneOutput {
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    name: String,
    #[serde(default)]
    needs_auth: bool,
    #[serde(default)]
    has_password: bool,
}

pub async fn fetch_owntone_outputs(base: &str) -> Result<Vec<CatalogDevice>, SenderError> {
    let http = crate::sender::sonos::soap::http_client();
    fetch_owntone_outputs_with_client(&http, base).await
}

pub(crate) async fn fetch_owntone_outputs_with_client(
    http: &Client,
    base: &str,
) -> Result<Vec<CatalogDevice>, SenderError> {
    let url = endpoint_url(base, "/api/outputs")?;
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| SenderError::transport(format!("OwnTone output catalog failed: {e}")))?;
    if !response.status().is_success() {
        return Err(SenderError::transport(format!(
            "OwnTone output catalog failed: {}",
            response.status()
        )));
    }
    let body = crate::http::read_bounded(response, MAX_CATALOG_BYTES)
        .await
        .map_err(|e| SenderError::transport(format!("OwnTone output catalog: {e}")))?;
    let body: OwnToneOutputs = serde_json::from_slice(&body)
        .map_err(|e| SenderError::transport(format!("invalid OwnTone output catalog: {e}")))?;
    Ok(body
        .outputs
        .into_iter()
        .map(|o| CatalogDevice {
            id: match o.id {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            },
            name: o.name,
            needs_pair: o.needs_auth || o.has_password,
            paired: !(o.needs_auth || o.has_password),
            kind: "solo",
            member_count: 1,
            address: String::new(),
        })
        .filter(|device| {
            !device.name.trim().is_empty() && !device.id.trim().is_empty() && device.id != "null"
        })
        .take(MAX_CATALOG_DEVICES)
        .collect())
}

pub async fn pair_owntone(base: &str, device_id: &str, pin: &str) -> Result<(), SenderError> {
    let http = crate::sender::sonos::soap::http_client();
    let pin_body = serde_json::json!({ "pin": pin, "pairing_code": pin });
    for path in ["/api/pairing", "/api/pair"] {
        let response = http
            .post(endpoint_url(base, path)?)
            .json(&pin_body)
            .send()
            .await;
        if let Ok(r) = response {
            if r.status().is_success() {
                return Ok(());
            }
        }
    }
    let selected = http
        .put(output_url(base, device_id)?)
        .json(&serde_json::json!({ "selected": true, "pin": pin }))
        .send()
        .await
        .map_err(|e| SenderError::transport(e.to_string()))?;
    if selected.status().is_success() {
        Ok(())
    } else {
        Err(SenderError::transport(format!(
            "AirPlay pair failed: {}",
            selected.status()
        )))
    }
}
