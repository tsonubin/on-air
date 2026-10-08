use crate::dsp::rates;
use crate::net::{local_lan_ip, local_lan_ip_toward};
use crate::sender::airplay::AirPlaySender;
use crate::sender::bluetooth::{
    BluetoothPlaybackConfig, BluetoothSender, PcmOutput, SystemPcmOutput,
};
use crate::sender::sonos::SonosSender;
use crate::sender::{AudioSender, NullSender, SenderError};
use crate::state::CoreState;
use std::fmt::Write as _;

mod output;

pub use output::{ActiveOutput, OutputPhase, OutputSession, OutputSnapshot};

#[derive(Debug, thiserror::Error)]
pub enum ActivateError {
    #[error("output device not found")]
    NotFound,
    #[error("unknown transport: {0}")]
    UnknownTransport(String),
    #[error("{0}")]
    Unsupported(&'static str),
    #[error("could not determine the LAN address for the audio stream: {0}")]
    NoLanAddress(std::io::Error),
    #[error("{0}")]
    Discovery(String),
    #[error(transparent)]
    Sender(#[from] SenderError),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OutputInfo {
    pub id: String,
    pub name: String,
    pub transport: &'static str,
    pub kind: &'static str,
    pub member_count: u8,
    pub needs_pair: bool,
    pub paired: bool,
}

pub async fn list(state: &CoreState) -> Vec<OutputInfo> {
    let mut outputs = Vec::new();
    for d in state.sonos.lock().await.list() {
        outputs.push(OutputInfo {
            id: d.usn,
            name: d.friendly_name,
            transport: "sonos",
            kind: if d.member_count >= 2 { "pair" } else { "solo" },
            member_count: d.member_count.max(1),
            needs_pair: false,
            paired: true,
        });
    }
    for d in state.airplay.lock().iter() {
        outputs.push(OutputInfo {
            id: d.id.clone(),
            name: d.name.clone(),
            transport: "airplay",
            kind: d.kind,
            member_count: d.member_count.max(1),
            needs_pair: d.needs_pair,
            paired: d.paired,
        });
    }
    let bluetooth_devices = state.bluetooth_devices().await.unwrap_or_default();
    for d in bluetooth_devices {
        outputs.push(OutputInfo {
            id: d.id,
            name: d.name,
            transport: "bluetooth",
            kind: "solo",
            member_count: 1,
            needs_pair: !d.paired,
            paired: d.paired,
        });
    }
    outputs
}

/// A fresh 32-hex-character path segment. Sonos and AirPlay receivers pull
/// `/stream/<nonce>/audio.wav`; the nonce changes on every activation so a
/// LAN host that saw an old URL cannot keep listening.
pub fn new_stream_nonce() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("operating-system randomness for stream nonce");
    let mut nonce = String::with_capacity(32);
    for byte in bytes {
        write!(&mut nonce, "{byte:02x}").expect("writing to a String cannot fail");
    }
    nonce
}

pub fn stream_url(lan_ip: std::net::IpAddr, nonce: &str) -> String {
    format!(
        "http://{lan_ip}:{}/stream/{nonce}/audio.wav",
        crate::DEFAULT_PORT
    )
}

struct Built {
    sender: Box<dyn AudioSender>,
    identity: ActiveOutput,
    stream_nonce: Option<String>,
}

pub async fn activate(
    state: &CoreState,
    transport: &str,
    device_id: &str,
) -> Result<(), ActivateError> {
    let built = match transport {
        "sonos" => build_sonos(state, device_id).await?,
        "airplay" => build_airplay(state, device_id)?,
        "bluetooth" => build_bluetooth(state, device_id).await?,
        _ => return Err(ActivateError::UnknownTransport(transport.to_string())),
    };
    let result = state
        .output()
        .activate(built.sender, Some(built.identity), built.stream_nonce)
        .await;
    if transport == "bluetooth" {
        state.invalidate_bluetooth_cache().await;
    }
    result?;
    if let Some(active) = state.output().active() {
        state.remember_output(active);
    }
    state.remember_sample_rates();
    let volume = state.output_volume();
    if let Err(error) = state.output().set_volume(volume).await {
        eprintln!("could not restore volume on the active {transport} output: {error}");
    }
    Ok(())
}

pub async fn set_volume(state: &CoreState, volume: u8) -> Result<(), ActivateError> {
    state.output().set_volume(volume).await?;
    state.remember_volume(volume);
    Ok(())
}

fn snap_output_rate(state: &CoreState, transport: &str) {
    let mut current = state.output_sample_rate_hz.lock();
    *current = rates::snap_rate(*current, rates::transport_rates(transport));
}

async fn build_sonos(state: &CoreState, device_id: &str) -> Result<Built, ActivateError> {
    snap_output_rate(state, "sonos");
    let device = {
        let registry = state.sonos.lock().await;
        registry.list().into_iter().find(|d| d.usn == device_id)
    };
    let Some(device) = device else {
        return Err(ActivateError::NotFound);
    };
    let identity = ActiveOutput {
        transport: "sonos".into(),
        device_id: device_id.to_string(),
        device_name: device.friendly_name.clone(),
    };
    let nonce = new_stream_nonce();
    if state.mock {
        return Ok(Built {
            sender: Box::new(NullSender::new(
                device.friendly_name,
                state.mock_log.clone(),
            )),
            identity,
            stream_nonce: Some(nonce),
        });
    }
    let lan_ip = local_lan_ip_toward(device.ip)
        .or_else(|_| local_lan_ip())
        .map_err(ActivateError::NoLanAddress)?;
    let url = stream_url(lan_ip, &nonce);
    Ok(Built {
        sender: Box::new(
            SonosSender::new(device, crate::sender::sonos::soap::http_client(), url)
                .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone()),
        ),
        identity,
        stream_nonce: Some(nonce),
    })
}

fn build_airplay(state: &CoreState, device_id: &str) -> Result<Built, ActivateError> {
    snap_output_rate(state, "airplay");
    if crate::sender::airplay::platform_mode() == "avroute-picker" && !state.mock {
        return Err(ActivateError::Unsupported(
            "macOS AirPlay is local-picker only",
        ));
    }
    let device = state
        .airplay
        .lock()
        .iter()
        .find(|d| d.id == device_id)
        .cloned();
    let Some(device) = device else {
        return Err(ActivateError::NotFound);
    };
    let identity = ActiveOutput {
        transport: "airplay".into(),
        device_id: device.id.clone(),
        device_name: device.name.clone(),
    };
    let nonce = new_stream_nonce();
    if state.mock {
        return Ok(Built {
            sender: Box::new(NullSender::new(device.name, state.mock_log.clone())),
            identity,
            stream_nonce: Some(nonce),
        });
    }
    let base = state.owntone_base.lock().clone();
    let peer: std::net::IpAddr = device
        .address
        .parse()
        .unwrap_or_else(|_| std::net::IpAddr::from([8, 8, 8, 8]));
    let lan_ip = local_lan_ip_toward(peer)
        .or_else(|_| local_lan_ip())
        .map_err(ActivateError::NoLanAddress)?;
    let url = stream_url(lan_ip, &nonce);
    Ok(Built {
        sender: Box::new(
            AirPlaySender::new(device.name, device.id, base).with_radio(url, device.address),
        ),
        identity,
        stream_nonce: Some(nonce),
    })
}

async fn build_bluetooth(state: &CoreState, device_id: &str) -> Result<Built, ActivateError> {
    let device = state
        .bluetooth_devices()
        .await
        .map_err(ActivateError::Discovery)?
        .into_iter()
        .find(|d| d.id == device_id || d.audio_endpoint.as_deref() == Some(device_id));
    let Some(device) = device else {
        return Err(ActivateError::NotFound);
    };
    let identity = ActiveOutput {
        transport: "bluetooth".into(),
        device_id: device.id.clone(),
        device_name: device.name.clone(),
    };
    let output: std::sync::Arc<dyn PcmOutput> = if state.mock {
        std::sync::Arc::new(state.pcm_sink.clone())
    } else {
        std::sync::Arc::new(SystemPcmOutput)
    };
    let config = BluetoothPlaybackConfig {
        pipeline_hz: *state.target_sample_rate_hz.lock(),
        output_hz: *state.output_sample_rate_hz.lock(),
        volume: state.output_volume(),
    };
    Ok(Built {
        sender: Box::new(BluetoothSender::new(
            device,
            state.bluetooth.clone(),
            state.audio_tx.clone(),
            output,
            config,
        )),
        identity,
        stream_nonce: None,
    })
}
