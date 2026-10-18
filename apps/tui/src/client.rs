//! Loopback HTTP client for the core. Routes, bodies and timeouts match
//! `packages/control-client/src/http.ts`; the TUI never needs a pairing
//! token because the core trusts loopback callers.

use crate::api::WsEvent;
use crate::api::{
    ActiveInputResponse, ActiveOutputResponse, ActiveOutputView, AirPlayModeResponse, ApiErrorBody,
    BluetoothDeviceInfo, BluetoothListResponse, CdAction, CdStatus, EqResponse, InputsResponse,
    OutputInfo, OutputVolumeResponse, OutputsResponse, PinResponse, SampleRateResponse,
    ServiceResponse, StatusResponse,
};
use crate::error_copy;
use crate::model::{MixerControl, Snapshot};
use crate::update::{self, Cmd, Msg};
use futures_util::StreamExt;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message;

/// Budget for reads and simple writes.
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Budget for calls that wait on a device handshake or on audio stopping.
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(60);

pub const DEFAULT_BASE: &str = "http://127.0.0.1:47990";

/// A socket with no frame for this long is treated as dead; the core sends
/// a heartbeat every 10 s.
pub const STALL: Duration = Duration::from_secs(20);
pub const MIN_BACKOFF: Duration = Duration::from_millis(500);
pub const MAX_BACKOFF: Duration = Duration::from_secs(10);
const BACKOFF_JITTER: f64 = 0.2;

/// Reconnect delay for the `attempt`-th consecutive failure: 500 ms doubling
/// to a 10 s cap, then ±20 % scaled by `jitter` in `[-1, 1]`.
pub fn backoff_delay(attempt: u32, jitter: f64) -> Duration {
    let base = MIN_BACKOFF
        .checked_mul(1u32 << attempt.min(16))
        .unwrap_or(MAX_BACKOFF)
        .min(MAX_BACKOFF);
    let factor = 1.0 + BACKOFF_JITTER * jitter.clamp(-1.0, 1.0);
    Duration::from_secs_f64(base.as_secs_f64() * factor)
}

/// `http(s)://host:port` → `ws(s)://host:port/api/ws`.
pub fn ws_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    let ws = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        format!("ws://{base}")
    };
    format!("{ws}/api/ws")
}

/// State of the event socket as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Open,
    Reconnecting,
    Closed,
}

/// What the event task sends the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Connection(Connection),
    Event(WsEvent),
}

/// Subscribe to `/api/ws` forever: `Connecting`, then `Open` on connect,
/// `Reconnecting` on close or stall, each parsed event in between. Frames
/// with a `type` this client does not know arrive as `WsEvent::Unknown`.
/// The task never ends on its own; abort the handle to stop it.
pub fn spawn_events(base: &str, tx: UnboundedSender<Incoming>) -> tokio::task::JoinHandle<()> {
    let url = ws_url(base);
    tokio::spawn(async move {
        let mut attempt: u32 = 0;
        let mut rng = Xorshift::seeded(std::process::id() as u64 ^ 0x9E37_79B9_7F4A_7C15);
        if tx
            .send(Incoming::Connection(Connection::Connecting))
            .is_err()
        {
            return;
        }
        loop {
            if let Ok((mut socket, _)) = tokio_tungstenite::connect_async(&url).await {
                {
                    attempt = 0;
                    if tx.send(Incoming::Connection(Connection::Open)).is_err() {
                        return;
                    }
                    loop {
                        let frame = tokio::time::timeout(STALL, socket.next()).await;
                        match frame {
                            Ok(Some(Ok(Message::Text(text)))) => {
                                match serde_json::from_str::<WsEvent>(&text) {
                                    Ok(event) => {
                                        if tx.send(Incoming::Event(event)).is_err() {
                                            return;
                                        }
                                    }
                                    Err(error) => eprintln!("on-air: bad event frame: {error}"),
                                }
                            }
                            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => break,
                            Ok(Some(Ok(_))) => {}
                            Err(_stall) => break,
                        }
                    }
                }
            }
            if tx
                .send(Incoming::Connection(Connection::Reconnecting))
                .is_err()
            {
                return;
            }
            tokio::time::sleep(backoff_delay(attempt, rng.unit())).await;
            attempt = attempt.saturating_add(1);
        }
    })
}

impl Client {
    /// Every part of the snapshot in parallel. A part that fails stays at
    /// its default and its copy is collected, except that a paused service
    /// is a state the header shows, not an error, so its 503s are dropped.
    /// A missing `status` means the core is gone; the caller reports that.
    pub async fn load_snapshot(&self) -> (Snapshot, Vec<String>) {
        let (
            status,
            pin,
            inputs,
            active_input,
            outputs,
            active_output,
            volume,
            eq,
            rates,
            cd,
            airplay_mode,
            bluetooth,
        ) = tokio::join!(
            self.status(),
            self.pin(),
            self.inputs(),
            self.active_input(),
            self.outputs(),
            self.active_output(),
            self.volume(),
            self.eq(),
            self.sample_rate(),
            self.cd(),
            self.airplay_mode(),
            self.bluetooth_devices(),
        );
        let mut failures: Vec<String> = Vec::new();
        let paused = matches!(&status, Ok(status) if !status.service_enabled);
        let snapshot = Snapshot {
            status: take_part(status, paused, &mut failures),
            pin: take_part(pin, paused, &mut failures),
            inputs: take_part(inputs, paused, &mut failures).unwrap_or_default(),
            active_input: take_part(active_input, paused, &mut failures).and_then(|r| r.name),
            outputs: take_part(outputs, paused, &mut failures).unwrap_or_default(),
            active_output: take_part(active_output, paused, &mut failures).flatten(),
            volume: take_part(volume, paused, &mut failures).unwrap_or_default(),
            eq: take_part(eq, paused, &mut failures).unwrap_or_default(),
            rates: take_part(rates, paused, &mut failures),
            cd: take_part(cd, paused, &mut failures),
            airplay_mode: take_part(airplay_mode, paused, &mut failures),
            // The list is optional: a failure shows as a line in the pane.
            bluetooth: bluetooth.ok(),
        };
        (snapshot, failures)
    }

    /// Execute one command and return the messages to feed back. `Quit`
    /// yields nothing; a refresh yields the snapshot and, when parts of it
    /// failed, a `Done` carrying their copy.
    pub async fn run_cmd(&self, cmd: Cmd) -> Vec<Msg> {
        let done = |label: &str, result: Result<(), ClientError>| {
            vec![Msg::Done {
                label: label.to_string(),
                result: result.map_err(|error| error_copy::copy(&error)),
            }]
        };
        let written = |control: MixerControl, result: Result<(), ClientError>| {
            vec![Msg::Written {
                control,
                result: result.map_err(|error| error_copy::copy(&error)),
            }]
        };
        match cmd {
            Cmd::Quit => vec![],
            Cmd::RunAudioCheck => vec![Msg::Benchmark(
                self.audio_check()
                    .await
                    .map_err(|error| error_copy::copy(&error)),
            )],
            Cmd::SetCaptureBuffer(buffer_ms) => done(
                update::LABEL_BUFFER,
                self.set_capture_buffer(buffer_ms).await,
            ),
            Cmd::RepairFirewall => {
                let result = self.repair_firewall().await;
                let mut messages = done(update::LABEL_FIREWALL, result);
                if matches!(messages.first(), Some(Msg::Done { result: Ok(()), .. })) {
                    let (snapshot, _) = self.load_snapshot().await;
                    messages.push(Msg::Snapshot(Ok(Box::new(snapshot))));
                } else if let Some(Msg::Done {
                    result: Err(copy), ..
                }) = messages.first_mut()
                {
                    *copy = "Firewall rule was not applied. Authorize on the host or run the displayed commands.".into();
                }
                messages
            }
            Cmd::Refresh => {
                let (snapshot, failures) = self.load_snapshot().await;
                if snapshot.status.is_none() {
                    let copy = failures.into_iter().next().unwrap_or_else(|| {
                        error_copy::copy(&ClientError::Unreachable(self.base.clone()))
                    });
                    return vec![Msg::Snapshot(Err(copy))];
                }
                let mut msgs = vec![Msg::Snapshot(Ok(Box::new(snapshot)))];
                if !failures.is_empty() {
                    msgs.push(Msg::Done {
                        label: update::LABEL_REFRESH.to_string(),
                        result: Err(failures.join(" ")),
                    });
                }
                msgs
            }
            Cmd::ActivateInput(name) => {
                done(update::LABEL_SOURCE, self.activate_input(&name).await)
            }
            Cmd::ActivateOutput {
                transport,
                device_id,
            } => done(
                update::LABEL_OUTPUT,
                self.activate_output(&transport, &device_id).await,
            ),
            Cmd::DeactivateOutput => done(update::LABEL_STOP, self.deactivate_output().await),
            Cmd::SetVolume(volume) => written(MixerControl::Volume, self.set_volume(volume).await),
            Cmd::SetEq(gains) => written(MixerControl::Eq, self.set_eq(gains).await),
            Cmd::SetSampleRate {
                input_hz,
                output_hz,
            } => {
                let control = if input_hz.is_some() {
                    MixerControl::InputRate
                } else {
                    MixerControl::OutputRate
                };
                written(control, self.set_sample_rate(input_hz, output_hz).await)
            }
            Cmd::LoadRates => vec![Msg::Rates(
                self.sample_rate()
                    .await
                    .map_err(|error| error_copy::copy(&error)),
            )],
            Cmd::ControlCd {
                action,
                position_ms,
                track,
            } => done(
                update::LABEL_CD,
                self.control_cd(action, position_ms, track).await,
            ),
            Cmd::PairAirplay { device_id, pin } => done(
                update::LABEL_PAIR,
                self.pair_airplay(&device_id, &pin).await,
            ),
            Cmd::PairBluetooth(id) => done(update::LABEL_PAIR, self.pair_bluetooth(&id).await),
            Cmd::ConnectBluetooth(id) => {
                done(update::LABEL_CONNECT, self.connect_bluetooth(&id).await)
            }
            Cmd::OpenBluetoothSettings => {
                // Open it from the TUI's own session when the core is local:
                // the user service may have no display to open a window on.
                if crate::handoff::is_loopback_base(&self.base)
                    && crate::handoff::open_bluetooth_settings()
                {
                    return done(update::LABEL_SETTINGS, Ok(()));
                }
                done(update::LABEL_SETTINGS, self.open_bluetooth_settings().await)
            }
            Cmd::SetService(enabled) => done(
                update::LABEL_MONITOR,
                self.set_service(enabled).await.map(|_| ()),
            ),
            Cmd::QueryAutostart => vec![Msg::Autostart(crate::autostart::is_enabled().await)],
            Cmd::SetAutostart(enabled) => {
                vec![Msg::Autostart(crate::autostart::set_enabled(enabled).await)]
            }
        }
    }
}

/// One part of the snapshot: its value, or `None` with its copy recorded.
/// A 503 while the service is paused is expected and not recorded.
fn take_part<T>(
    result: Result<T, ClientError>,
    paused: bool,
    failures: &mut Vec<String>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(ClientError::Http { status: 503, .. }) if paused => None,
        Err(error) => {
            let copy = error_copy::copy(&error);
            if !failures.contains(&copy) {
                failures.push(copy);
            }
            None
        }
    }
}

/// Enough randomness for reconnect jitter without a dependency.
struct Xorshift(u64);

impl Xorshift {
    fn seeded(seed: u64) -> Self {
        Xorshift(if seed == 0 { 1 } else { seed })
    }

    /// A value in `[-1, 1]`.
    fn unit(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// No response at all: the core is down or the port is closed.
    Unreachable(String),
    Timeout,
    /// A non-2xx response; `code` and `message` come from the envelope.
    Http {
        status: u16,
        code: Option<String>,
        message: String,
    },
}

#[derive(Clone)]
pub struct Client {
    base: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base: &str) -> Self {
        Client {
            base: base.trim_end_matches('/').to_string(),
            // The core is local: a system HTTP_PROXY must not intercept it.
            http: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("reqwest core client"),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<impl Serialize>,
        timeout: Duration,
    ) -> Result<T, ClientError> {
        let url = format!("{}{}", self.base, path);
        let mut request = self.http.request(method, &url).timeout(timeout);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                ClientError::Timeout
            } else {
                ClientError::Unreachable(self.base.clone())
            }
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|error| {
            if error.is_timeout() {
                ClientError::Timeout
            } else {
                ClientError::Unreachable(self.base.clone())
            }
        })?;
        if !status.is_success() {
            let envelope = serde_json::from_slice::<ApiErrorBody>(&bytes).ok();
            return Err(ClientError::Http {
                status: status.as_u16(),
                code: envelope.as_ref().map(|e| e.code.clone()),
                message: envelope
                    .map(|e| e.error)
                    .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned()),
            });
        }
        // Empty and 204 replies deserialize to `()`; everything else is JSON.
        let text = if bytes.is_empty() { "null" } else { "" };
        let value = if text.is_empty() {
            serde_json::from_slice::<T>(&bytes)
        } else {
            serde_json::from_str::<T>(text)
        };
        value.map_err(|error| ClientError::Http {
            status: status.as_u16(),
            code: None,
            message: format!("unexpected reply from {path}: {error}"),
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        self.request(Method::GET, path, None::<()>, READ_TIMEOUT)
            .await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: impl Serialize,
        timeout: Duration,
    ) -> Result<T, ClientError> {
        self.request(method, path, Some(body), timeout).await
    }

    pub async fn status(&self) -> Result<StatusResponse, ClientError> {
        self.get("/api/status").await
    }

    pub async fn audio_check(&self) -> Result<crate::api::AudioBenchmark, ClientError> {
        self.send_json(
            Method::POST,
            "/api/audio/benchmark",
            json!({}),
            Duration::from_secs(15),
        )
        .await
    }

    pub async fn repair_firewall(&self) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/network/firewall",
            json!({}),
            Duration::from_secs(125),
        )
        .await
    }

    pub async fn set_capture_buffer(&self, buffer_ms: u32) -> Result<(), ClientError> {
        self.send_json(
            Method::PUT,
            "/api/audio/buffer",
            json!({"buffer_ms": buffer_ms}),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn pin(&self) -> Result<String, ClientError> {
        self.get::<PinResponse>("/api/pairing/pin")
            .await
            .map(|r| r.pin)
    }

    pub async fn inputs(&self) -> Result<Vec<String>, ClientError> {
        self.get::<InputsResponse>("/api/inputs")
            .await
            .map(|r| r.inputs)
    }

    pub async fn active_input(&self) -> Result<ActiveInputResponse, ClientError> {
        self.get("/api/inputs/active").await
    }

    pub async fn activate_input(&self, name: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/inputs/active",
            json!({ "name": name }),
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn outputs(&self) -> Result<Vec<OutputInfo>, ClientError> {
        self.get::<OutputsResponse>("/api/outputs")
            .await
            .map(|r| r.outputs)
    }

    pub async fn active_output(&self) -> Result<Option<ActiveOutputView>, ClientError> {
        self.get::<Option<ActiveOutputResponse>>("/api/outputs/active")
            .await
            .map(|r| r.and_then(|r| r.active))
    }

    pub async fn activate_output(
        &self,
        transport: &str,
        device_id: &str,
    ) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/outputs/active",
            json!({ "transport": transport, "device_id": device_id }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn deactivate_output(&self) -> Result<(), ClientError> {
        self.request(
            Method::DELETE,
            "/api/outputs/active",
            None::<()>,
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn volume(&self) -> Result<u8, ClientError> {
        self.get::<OutputVolumeResponse>("/api/outputs/active/volume")
            .await
            .map(|r| r.volume)
    }

    pub async fn set_volume(&self, volume: u8) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/outputs/active/volume",
            json!({ "volume": volume }),
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn eq(&self) -> Result<[f32; 5], ClientError> {
        self.get::<EqResponse>("/api/eq").await.map(|r| r.gains_db)
    }

    pub async fn set_eq(&self, gains_db: [f32; 5]) -> Result<(), ClientError> {
        self.send_json(
            Method::PUT,
            "/api/eq",
            json!({ "gains_db": gains_db }),
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn sample_rate(&self) -> Result<SampleRateResponse, ClientError> {
        self.get("/api/sample-rate").await
    }

    pub async fn set_sample_rate(
        &self,
        input_hz: Option<u32>,
        output_hz: Option<u32>,
    ) -> Result<(), ClientError> {
        let mut body = serde_json::Map::new();
        if let Some(hz) = input_hz {
            body.insert("input_hz".into(), hz.into());
        }
        if let Some(hz) = output_hz {
            body.insert("output_hz".into(), hz.into());
        }
        self.send_json(Method::PUT, "/api/sample-rate", body, READ_TIMEOUT)
            .await
    }

    pub async fn airplay_mode(&self) -> Result<String, ClientError> {
        self.get::<AirPlayModeResponse>("/api/airplay/mode")
            .await
            .map(|r| r.mode)
    }

    pub async fn pair_airplay(&self, device_id: &str, pin: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/airplay/pair",
            json!({ "device_id": device_id, "pin": pin }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn bluetooth_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, ClientError> {
        self.get::<BluetoothListResponse>("/api/bluetooth/devices")
            .await
            .map(|r| r.devices)
    }

    pub async fn pair_bluetooth(&self, id: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/bluetooth/pair",
            json!({ "id": id }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn connect_bluetooth(&self, id: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/bluetooth/connect",
            json!({ "id": id }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn open_bluetooth_settings(&self) -> Result<(), ClientError> {
        self.request(
            Method::POST,
            "/api/bluetooth/settings",
            None::<()>,
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn cd(&self) -> Result<CdStatus, ClientError> {
        self.get("/api/cd").await
    }

    pub async fn control_cd(
        &self,
        action: CdAction,
        position_ms: Option<u64>,
        track: Option<u8>,
    ) -> Result<(), ClientError> {
        let mut body = json!({ "action": action });
        if let Some(position_ms) = position_ms {
            body["position_ms"] = position_ms.into();
        }
        if let Some(track) = track {
            body["track"] = track.into();
        }
        self.send_json::<CdStatus>(Method::POST, "/api/cd/control", body, READ_TIMEOUT)
            .await
            .map(|_| ())
    }

    pub async fn set_service(&self, enabled: bool) -> Result<bool, ClientError> {
        self.send_json::<ServiceResponse>(
            Method::PUT,
            "/api/service",
            json!({ "enabled": enabled }),
            SLOW_TIMEOUT,
        )
        .await
        .map(|r| r.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MeterLevel;

    #[test]
    fn a_trailing_slash_is_trimmed_from_the_base() {
        assert_eq!(Client::new("http://x:1/").base(), "http://x:1");
    }

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_delay(0, 0.0), Duration::from_millis(500));
        assert_eq!(backoff_delay(1, 0.0), Duration::from_millis(1000));
        assert_eq!(backoff_delay(10, 0.0), Duration::from_secs(10));
        assert_eq!(backoff_delay(0, 1.0), Duration::from_millis(600));
        assert_eq!(backoff_delay(0, -1.0), Duration::from_millis(400));
    }

    #[test]
    fn the_events_url_is_the_ws_form_of_the_base() {
        assert_eq!(
            ws_url("http://127.0.0.1:47990"),
            "ws://127.0.0.1:47990/api/ws"
        );
        assert_eq!(ws_url("https://host"), "wss://host/api/ws");
    }

    #[test]
    fn level_meter_frame_decodes() {
        let event = serde_json::from_str::<WsEvent>(
            r#"{"type":"LevelMeter","left":{"rms":0.25,"peak":0.5},"right":{"rms":0.1,"peak":0.2}}"#,
        )
        .unwrap();
        assert_eq!(
            event,
            WsEvent::LevelMeter {
                left: MeterLevel {
                    rms: 0.25,
                    peak: 0.5
                },
                right: MeterLevel {
                    rms: 0.1,
                    peak: 0.2
                },
            }
        );
    }
}
