use crate::dsp::rates;
use crate::sender::airplay::AirPlaySender;
use crate::sender::bluetooth::{
    BluetoothPlaybackConfig, BluetoothSender, PcmOutput, SystemPcmOutput,
};
use crate::sender::sonos::{net::local_lan_ip, SonosSender};
use crate::sender::{AudioSender, NullSender, SenderError};
use crate::state::{ActiveOutput, CoreState};

#[derive(Debug)]
pub enum ActivateError {
    NotFound,
    BadRequest(&'static str),
    Failed(String),
}

impl From<SenderError> for ActivateError {
    fn from(e: SenderError) -> Self {
        ActivateError::Failed(e.0)
    }
}

#[derive(Debug, Clone)]
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
    for d in state.outputs.lock().await.list() {
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
    for d in state.airplay_outputs.lock().unwrap().iter() {
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

pub async fn activate(
    state: &CoreState,
    transport: &str,
    device_id: &str,
) -> Result<(), ActivateError> {
    let (sender, identity) = match transport {
        "sonos" => build_sonos(state, device_id).await?,
        "airplay" => build_airplay(state, device_id)?,
        "bluetooth" => build_bluetooth(state, device_id).await?,
        _ => return Err(ActivateError::BadRequest("unknown transport")),
    };
    let result = state.activate_sender_as(sender, Some(identity)).await;
    if transport == "bluetooth" {
        state.invalidate_bluetooth_cache().await;
    }
    result.map_err(ActivateError::from)?;
    let active = state.active_output.lock().unwrap().clone();
    if let Some(active) = active {
        state.remember_output(active);
    }
    state.remember_sample_rates();
    let volume = state
        .output_volume
        .load(std::sync::atomic::Ordering::Acquire);
    if let Err(error) = set_active_sender_volume(state, volume).await {
        eprintln!("could not restore volume on the active {transport} output: {error}");
    }
    Ok(())
}

pub async fn set_volume(state: &CoreState, volume: u8) -> Result<(), ActivateError> {
    set_active_sender_volume(state, volume)
        .await
        .map_err(ActivateError::from)?;
    state.remember_volume(volume);
    Ok(())
}

async fn set_active_sender_volume(state: &CoreState, volume: u8) -> Result<(), SenderError> {
    let mut guard = state.active_sender.lock().await;
    match guard.as_mut() {
        Some(sender) => sender.set_volume(volume).await,
        None => Err(SenderError("no active output".into())),
    }
}

fn snap_output_rate(state: &CoreState, transport: &str) {
    let mut current = state.output_sample_rate_hz.lock().unwrap();
    *current = rates::snap_rate(*current, rates::transport_rates(transport));
}

async fn build_sonos(
    state: &CoreState,
    device_id: &str,
) -> Result<(Box<dyn AudioSender>, ActiveOutput), ActivateError> {
    snap_output_rate(state, "sonos");
    let device = {
        let registry = state.outputs.lock().await;
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
    if state.mock {
        return Ok((
            Box::new(NullSender::new(
                device.friendly_name,
                state.mock_log.clone(),
            )),
            identity,
        ));
    }
    let lan_ip = crate::sender::sonos::net::local_lan_ip_toward(device.ip)
        .or_else(|_| local_lan_ip())
        .map_err(|e| ActivateError::Failed(e.to_string()))?;
    let stream_url = format!("http://{lan_ip}:{}/stream/audio.wav", crate::DEFAULT_PORT);
    Ok((
        Box::new(
            SonosSender::new(
                device,
                crate::sender::sonos::soap::http_client(),
                stream_url,
            )
            .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone()),
        ),
        identity,
    ))
}

fn build_airplay(
    state: &CoreState,
    device_id: &str,
) -> Result<(Box<dyn AudioSender>, ActiveOutput), ActivateError> {
    snap_output_rate(state, "airplay");
    if crate::sender::airplay::platform_mode() == "avroute-picker" && !state.mock {
        return Err(ActivateError::BadRequest(
            "macOS AirPlay is local-picker only",
        ));
    }
    let device = state
        .airplay_outputs
        .lock()
        .unwrap()
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
    if state.mock {
        return Ok((
            Box::new(NullSender::new(device.name, state.mock_log.clone())),
            identity,
        ));
    }
    let base = state.owntone_base.lock().unwrap().clone();
    let peer: std::net::IpAddr = device
        .address
        .parse()
        .unwrap_or_else(|_| std::net::IpAddr::from([8, 8, 8, 8]));
    let lan_ip = crate::sender::sonos::net::local_lan_ip_toward(peer)
        .or_else(|_| crate::sender::sonos::net::local_lan_ip())
        .map_err(|e| ActivateError::Failed(e.to_string()))?;
    let stream_url = format!("http://{lan_ip}:{}/stream/audio.wav", crate::DEFAULT_PORT);
    Ok((
        Box::new(
            AirPlaySender::new(device.name, device.id, base).with_radio(stream_url, device.address),
        ),
        identity,
    ))
}

async fn build_bluetooth(
    state: &CoreState,
    device_id: &str,
) -> Result<(Box<dyn AudioSender>, ActiveOutput), ActivateError> {
    let device = state
        .bluetooth_devices()
        .await
        .map_err(ActivateError::Failed)?
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
        pipeline_hz: *state.target_sample_rate_hz.lock().unwrap(),
        output_hz: *state.output_sample_rate_hz.lock().unwrap(),
        volume: state
            .output_volume
            .load(std::sync::atomic::Ordering::Acquire),
    };
    Ok((
        Box::new(BluetoothSender::new(
            device,
            state.bluetooth.clone(),
            state.audio_tx.clone(),
            output,
            config,
        )),
        identity,
    ))
}
