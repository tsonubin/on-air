//! Disc insert/eject orchestration: an inserted Audio CD takes over as the
//! live input and plays from track one; ejecting it clears the input.

use super::{eject_drive, probe_audio_cd, CdMediaEvent, AUDIO_CD_INPUT};
use crate::state::CoreState;
use std::sync::atomic::Ordering;
use std::time::Duration;

const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// A disc was inserted: take the input and start playback.
pub async fn autoplay(state: &CoreState) {
    if !state.service_enabled.load(Ordering::Acquire) {
        return;
    }
    if !state.cd.status().present {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    if let Err(error) = state.activate_input_locked(AUDIO_CD_INPUT).await {
        eprintln!("cd autoplay could not take the input: {error}");
        return;
    }
    state.cd.play();
}

/// The disc left the drive: if it was the live input, stop capturing and
/// forget it as the saved input.
pub async fn on_ejected(state: &CoreState) {
    if state.input().active_name().as_deref() != Some(AUDIO_CD_INPUT) {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    if state.input().stop_if_active(AUDIO_CD_INPUT).await {
        state.clear_saved_input();
    }
}

/// Eject the drive (on real hardware) and clear the CD input.
pub async fn eject(state: &CoreState) {
    if !state.mock {
        // The eject ioctl blocks while the tray moves; keep it off the
        // async workers.
        let _ = tokio::task::spawn_blocking(eject_drive).await;
    }
    let _ = state.cd.eject();
    on_ejected(state).await;
}

/// Play pressed while another input is live: switch to the CD first.
pub async fn ensure_capture(state: &CoreState) {
    if state.input().active_name().as_deref() == Some(AUDIO_CD_INPUT) {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    if let Err(error) = state.activate_input_locked(AUDIO_CD_INPUT).await {
        eprintln!("cd play could not take the input: {error}");
    }
}

/// Apply a media change reported by the drive or the mock simulator.
pub async fn on_media_event(state: &CoreState, event: CdMediaEvent) {
    match event {
        CdMediaEvent::Inserted => autoplay(state).await,
        CdMediaEvent::Ejected => on_ejected(state).await,
        CdMediaEvent::Unchanged => {}
    }
}

/// Poll the optical drive. Only started for a non-mock state.
pub fn spawn_watch(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut empty_probes = 0u8;
        loop {
            let found = tokio::task::spawn_blocking(probe_audio_cd)
                .await
                .ok()
                .flatten();
            if found.is_none() {
                empty_probes = empty_probes.saturating_add(1);
                if empty_probes < 3 {
                    tokio::time::sleep(WATCH_INTERVAL).await;
                    continue;
                }
            } else {
                empty_probes = 0;
            }
            let event = state.cd.set_medium(found);
            on_media_event(&state, event).await;
            tokio::time::sleep(WATCH_INTERVAL).await;
        }
    })
}
