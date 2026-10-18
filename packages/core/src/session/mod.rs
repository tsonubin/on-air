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
    #[error("{0}")]
    Feedback(&'static str),
    #[error(transparent)]
    Sender(#[from] SenderError),
}

pub use on_air_api::OutputInfo;

/// Why System audio cannot play on this computer.
pub const FEEDBACK_LOOP: &str =
    "System audio already plays on this computer. Choose a capture input or another speaker.";

pub async fn list(state: &CoreState) -> Vec<OutputInfo> {
    let mut outputs = Vec::new();
    if !state.mock && !state.patchbay_endpoint {
        let local = tokio::task::spawn_blocking(crate::sender::local::query)
            .await
            .unwrap_or_default();
        for d in local {
            outputs.push(OutputInfo {
                id: d.id,
                name: d.name,
                transport: crate::sender::local::TRANSPORT.into(),
                kind: "solo".into(),
                member_count: 1,
                needs_pair: false,
                paired: true,
            });
        }
    }
    for d in state.sonos.lock().await.list() {
        outputs.push(OutputInfo {
            id: d.usn,
            name: d.friendly_name,
            transport: "sonos".into(),
            kind: (if d.member_count >= 2 { "pair" } else { "solo" }).into(),
            member_count: d.member_count.max(1),
            needs_pair: false,
            paired: true,
        });
    }
    for d in state.airplay.lock().iter() {
        outputs.push(OutputInfo {
            id: d.id.clone(),
            name: d.name.clone(),
            transport: "airplay".into(),
            kind: d.kind.into(),
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
            transport: "bluetooth".into(),
            kind: "solo".into(),
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

pub fn stream_url(lan_ip: std::net::IpAddr, port: u16, nonce: &str) -> String {
    format!("http://{lan_ip}:{port}/stream/{nonce}/audio.wav")
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
    let _senders = state.sender_lock.lock().await;
    if !state
        .service_enabled
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(SenderError::not_ready("The on-air service is paused.").into());
    }
    let reserved = if state.patchbay_endpoint {
        state
            .legacy_output
            .as_ref()
            .and_then(OutputSession::active)
            .is_some_and(|active| active.transport == transport && active.device_id == device_id)
    } else {
        state.patchbay.uses_receiver(transport, device_id).await
    };
    if reserved {
        return Err(SenderError::not_ready(
            "This speaker is in use by another on-air session. Disconnect its route first.",
        )
        .into());
    }
    let built = match transport {
        "sonos" => build_sonos(state, device_id).await?,
        "airplay" => build_airplay(state, device_id)?,
        "bluetooth" => build_bluetooth(state, device_id).await?,
        "local" => build_local(state, device_id).await?,
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
    let url = state.stream_url(lan_ip, &nonce);
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
    let url = state.stream_url(lan_ip, &nonce);
    Ok(Built {
        sender: Box::new(
            AirPlaySender::new(device.name, device.id, base)
                .with_radio(url, device.address)
                .with_independent_stream(state.patchbay_endpoint),
        ),
        identity,
        stream_nonce: Some(nonce),
    })
}

async fn build_local(state: &CoreState, device_id: &str) -> Result<Built, ActivateError> {
    if state.mock || state.patchbay_endpoint {
        return Err(ActivateError::NotFound);
    }
    if state.input().active_name().as_deref() == Some(crate::pipeline::sources::SYSTEM_AUDIO) {
        return Err(ActivateError::Feedback(FEEDBACK_LOOP));
    }
    snap_output_rate(state, "local");
    let output = tokio::task::spawn_blocking(crate::sender::local::query)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|o| o.id == device_id)
        .ok_or(ActivateError::NotFound)?;
    let adapter = crate::sender::local::LocalPortAdapter::new(output);
    let device = adapter.device();
    let identity = ActiveOutput {
        transport: "local".into(),
        device_id: device.id.clone(),
        device_name: device.name.clone(),
    };
    let config = BluetoothPlaybackConfig {
        pipeline_hz: *state.target_sample_rate_hz.lock(),
        output_hz: *state.output_sample_rate_hz.lock(),
        volume: state.output_volume(),
        dsp_quality: state.dsp_quality(),
    };
    Ok(Built {
        sender: Box::new(
            BluetoothSender::new(
                device,
                adapter,
                state.audio_tx.clone(),
                std::sync::Arc::new(SystemPcmOutput),
                config,
            )
            .with_transport("local"),
        ),
        identity,
        stream_nonce: None,
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
        dsp_quality: state.dsp_quality(),
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
