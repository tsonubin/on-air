#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn pyatv_helper_applies_volume_changes_while_streaming() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("tests/fixtures/pyatv");
    let script = manifest.join("src/sender/airplay_play.py");
    let mut child = Command::new("python3")
        .arg(script)
        .args([
            "--host",
            "127.0.0.1",
            "--url",
            "http://127.0.0.1/stream/audio.wav",
            "--volume",
            "22",
        ])
        .env("PYTHONPATH", fixture)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("python3 must be available for the AirPlay helper");

    let stdout = child.stdout.take().unwrap();
    let (lines_tx, lines_rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = lines_tx.send(line);
        }
    });

    let initial_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut initial = None;
    while let Some(remaining) =
        initial_deadline.checked_duration_since(std::time::Instant::now())
    {
        match lines_rx.recv_timeout(remaining) {
            Ok(line) if line == "volume:22" => {
                initial = Some(line);
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert_eq!(initial.as_deref(), Some("volume:22"));
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"73\n").unwrap();
    stdin.flush().unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut changed = None;
    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match lines_rx.recv_timeout(remaining) {
            Ok(line) if line == "volume:73" => {
                changed = Some(line);
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(changed.as_deref(), Some("volume:73"));
}
