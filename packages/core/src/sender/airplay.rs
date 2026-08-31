use crate::sender::{AudioSender, SenderError};
use reqwest::Client;
use std::process::{Child, Command, Stdio};

/// OwnTone sidecar when present; otherwise pyatv RAOP to the receiver's IP.
pub struct AirPlaySender {
    name: String,
    output_id: String,
    base_url: String,
    stream_url: String,
    host: String,
    http: Client,
    child: Option<Child>,
}

impl AirPlaySender {
    pub fn new(name: impl Into<String>, output_id: impl Into<String>, base_url: impl Into<String>) -> Self {
        AirPlaySender {
            name: name.into(),
            output_id: output_id.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            stream_url: String::new(),
            host: String::new(),
            http: crate::sender::sonos::soap::http_client(),
            child: None,
        }
    }

    pub fn with_radio(mut self, stream_url: impl Into<String>, host: impl Into<String>) -> Self {
        self.stream_url = stream_url.into();
        self.host = host.into();
        self
    }

    async fn put_json(&self, path: &str, body: serde_json::Value) -> Result<(), SenderError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .http
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SenderError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(SenderError(format!(
                "OwnTone {} failed: {}",
                path,
                response.status()
            )))
        }
    }

    async fn owntone_reachable(&self) -> bool {
        let url = format!("{}/api/outputs", self.base_url);
        let probe = crate::sender::sonos::soap::lan_client(std::time::Duration::from_millis(400));
        probe
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn start_owntone(&self) -> Result<(), SenderError> {
        self.put_json(
            &format!("/api/outputs/{}", self.output_id),
            serde_json::json!({ "selected": true }),
        )
        .await?;
        if !self.stream_url.is_empty() {
            let add = format!(
                "{}/api/queue/items/add?uris={}",
                self.base_url,
                urlencoding_loose(&self.stream_url)
            );
            let _ = self.http.post(&add).send().await;
        }
        self.put_json("/api/player/play", serde_json::json!({})).await
    }

    fn start_pyatv(&mut self) -> Result<(), SenderError> {
        if self.host.is_empty() || self.stream_url.is_empty() {
            return Err(SenderError(format!(
                "OwnTone is not running at {} (Linux AirPlay sidecar)",
                self.base_url
            )));
        }
        let script = std::env::temp_dir().join("on-air-airplay-play.py");
        std::fs::write(&script, include_str!("airplay_play.py"))
            .map_err(|e| SenderError(format!("write airplay helper: {e}")))?;
        let mut cmd = pyatv_command()?;
        cmd.arg(&script)
            .arg("--host")
            .arg(&self.host)
            .arg("--url")
            .arg(&self.stream_url)
            .arg("--volume")
            .arg("22")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd
            .spawn()
            .map_err(|e| SenderError(format!("start AirPlay helper: {e}")))?;
        self.child = Some(child);
        Ok(())
    }

    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            {
                let pid = child.id();
                let _ = Command::new("kill")
                    .args(["-TERM", "--", &format!("-{pid}")])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn urlencoding_loose(url: &str) -> String {
    url.replace(':', "%3A").replace('/', "%2F")
}

fn pyatv_command() -> Result<Command, SenderError> {
    if python_has_pyatv("python3") {
        return Ok(Command::new("python3"));
    }
    for uv in ["uv", "/home/linuxbrew/.linuxbrew/bin/uv", "/usr/bin/uv"] {
        if command_ok(uv, &["--version"]) {
            let mut cmd = Command::new(uv);
            cmd.args(["run", "--with", "pyatv", "--python", "python3"]);
            return Ok(cmd);
        }
    }
    Err(SenderError(
        "AirPlay needs OwnTone on :3689 or pyatv (`pip install pyatv` / `uv run --with pyatv`)".into(),
    ))
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

fn command_ok(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[async_trait::async_trait]
impl AudioSender for AirPlaySender {
    async fn start(&mut self) -> Result<(), SenderError> {
        if self.owntone_reachable().await {
            self.start_owntone().await
        } else {
            self.start_pyatv()
        }
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        if self.child.is_some() {
            self.stop_child();
            return Ok(());
        }
        let _ = self.put_json("/api/player/stop", serde_json::json!({})).await;
        Ok(())
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        if self.child.is_some() {
            return Ok(());
        }
        self.put_json(
            "/api/player/volume",
            serde_json::json!({ "volume": volume.min(100) }),
        )
        .await
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn transport(&self) -> &'static str {
        "airplay"
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
    #[serde(default)]
    r#type: String,
}

pub async fn fetch_owntone_outputs(base: &str) -> Result<Vec<crate::state::CatalogDevice>, SenderError> {
    let url = format!("{}/api/outputs", base.trim_end_matches('/'));
    let body: OwnToneOutputs = crate::sender::sonos::soap::http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| SenderError(e.to_string()))?
        .json()
        .await
        .map_err(|e| SenderError(e.to_string()))?;
    Ok(body
        .outputs
        .into_iter()
        .map(|o| crate::state::CatalogDevice {
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
        .filter(|d| !d.name.is_empty())
        .collect())
}

pub async fn pair_owntone(base: &str, device_id: &str, pin: &str) -> Result<(), SenderError> {
    let http = crate::sender::sonos::soap::http_client();
    let base = base.trim_end_matches('/');
    let pin_body = serde_json::json!({ "pin": pin, "pairing_code": pin });
    for path in ["/api/pairing", "/api/pair"] {
        let response = http
            .post(format!("{base}{path}"))
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
        .put(format!("{base}/api/outputs/{device_id}"))
        .json(&serde_json::json!({ "selected": true, "pin": pin }))
        .send()
        .await
        .map_err(|e| SenderError(e.to_string()))?;
    if selected.status().is_success() {
        Ok(())
    } else {
        Err(SenderError(format!(
            "AirPlay pair failed: {}",
            selected.status()
        )))
    }
}
