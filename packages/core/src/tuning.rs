//! Automatic first-use tuning: one measured configuration pass per
//! machine, so a fresh install gets a preset without the user finding the
//! audio-check button (roadmap C6).
//!
//! # Trigger
//!
//! The trigger is [`SavedSettings::recommended_dsp_quality`] being `None`:
//! "no audio check has ever completed on this machine". Every completed
//! check — the automatic one here or the manual button — sets it, so
//! tuning runs **once per machine, ever**, and a second boot does not
//! repeat it. On top of that signal, [`decide`] requires a live capture
//! (tuning must not run before capture can work), an unpaused service and
//! a real (non-mock) core. Concretely:
//!
//! - **Fresh install** (no settings file): everything is default, the
//!   signal is `None`, and tuning runs shortly after the first capture
//!   goes live.
//! - **Settings file from an older version**: the new fields are absent
//!   and read as `None` ([`SavedSettings`]' `serde(default)`), so tuning
//!   runs exactly like a fresh install. No migration is needed.
//! - **A machine where a human already found the check button**: the
//!   manual check sets the signal, so automatic tuning never runs — the
//!   human has already seen the verdict.
//! - **Manually-set `dsp_quality` / `capture_buffer_ms`**: tuning still
//!   runs once (to measure and record the machine's verdict) but applies
//!   nothing to the settings the user chose — see the apply rule.
//! - **Mock mode, no capture device, or a paused service**: skipped
//!   cleanly and cheaply, with the signal left `None` so a later boot (or
//!   a later capture) retries. Startup never fails because tuning failed.
//! - **A failed check** (pipeline did not respond, benchmark errored):
//!   nothing is recorded; the signal stays `None` and the next capture
//!   transition (typically the next boot) retries.
//!
//! # Apply rule
//!
//! "Preconfigure" means applying, but silently overwriting a user's choice
//! would be surprising. The rule: **the recommendation is applied only to
//! settings that are still untouched** — `capture_buffer_ms == 0` (the
//! sentinel every pre-benchmark install has) and `dsp_quality == None`
//! (never chosen). Anything the user has set is left alone. What ran and
//! what was applied is recorded in
//! [`FirstRunTuning`](crate::settings::FirstRunTuning) so the user can see
//! it (the TUI's audio-check surface) and override it later
//! (`PUT /api/audio/quality`, `PUT /api/audio/buffer`).
use crate::audio_benchmark::{self, BenchmarkOutcome};
use crate::dsp::DspQuality;
use crate::settings::{FirstRunTuning, SavedSettings};
use crate::state::CoreState;
use std::sync::atomic::Ordering;

/// The isolated check's iteration budget for the automatic run: the same
/// one the manual button uses; `run` stops early once three seconds of
/// real pipeline work have been measured.
const TUNING_ITERATIONS: usize = 96;

/// What to do about first-use tuning right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningAction {
    /// A check is pending and everything it needs is present: run it.
    Run,
    /// Not now, but try again at the next opportunity (the signal stays
    /// `None`).
    Wait(&'static str),
    /// Never here (already tuned) or never in this mode (mock).
    Never(&'static str),
}

/// The exact first-use trigger. See the module docs for the transitions.
pub fn decide(
    settings: &SavedSettings,
    service_enabled: bool,
    mock: bool,
    capturing: bool,
) -> TuningAction {
    if settings.recommended_dsp_quality.is_some() {
        return TuningAction::Never("a check has already completed on this machine");
    }
    if mock {
        return TuningAction::Never("mock mode has no audio hardware to tune");
    }
    if !service_enabled {
        return TuningAction::Wait("the service is paused");
    }
    if !capturing {
        return TuningAction::Wait("no capture is live yet");
    }
    TuningAction::Run
}

/// What a completed automatic check changed in the saved settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppliedTuning {
    pub buffer: bool,
    pub quality: bool,
}

impl AppliedTuning {
    pub fn any(&self) -> bool {
        self.buffer || self.quality
    }
}

/// Fold a completed automatic check's verdict into the saved settings:
/// record it (the trigger flips to "tuned") and apply it only where the
/// setting is still untouched (see the module docs). The applied flags are
/// recorded in `first_run_tuning`.
pub fn apply_verdict(
    saved: &mut SavedSettings,
    recommended_buffer_ms: u32,
    recommended_quality: DspQuality,
    now_unix_s: u64,
) -> AppliedTuning {
    let buffer_untouched = saved.capture_buffer_ms == 0;
    let quality_untouched = saved.dsp_quality.is_none();
    if buffer_untouched {
        saved.capture_buffer_ms = recommended_buffer_ms;
    }
    if quality_untouched {
        saved.dsp_quality = Some(recommended_quality);
    }
    saved.recommended_dsp_quality = Some(recommended_quality);
    saved.first_run_tuning = Some(FirstRunTuning {
        ran_at_unix_s: now_unix_s,
        recommended_buffer_ms,
        applied_buffer: buffer_untouched,
        applied_quality: quality_untouched,
    });
    AppliedTuning {
        buffer: buffer_untouched,
        quality: quality_untouched,
    }
}

/// Unix seconds, for the tuning record's timestamp.
pub(crate) fn now_unix_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Watch for the first usable capture and run the automatic check there.
/// Cheap to start (one settings read per capture transition) and exits as
/// soon as a check has completed on this machine, so it never runs again
/// on later boots. See the module docs for the trigger and apply rule.
pub fn spawn_first_run_watcher(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut input = state.input().subscribe();
        loop {
            let snapshot = input.borrow_and_update().clone();
            let capturing = snapshot.name.is_some() && snapshot.error.is_none();
            let action = decide(
                &state.saved_settings(),
                state.service_enabled.load(Ordering::Acquire),
                state.mock,
                capturing,
            );
            match action {
                TuningAction::Never("mock mode has no audio hardware to tune") => {
                    eprintln!("first-run audio tuning skipped: mock mode has no audio hardware to tune");
                    return;
                }
                TuningAction::Never(_) => return,
                TuningAction::Run => {
                    if run_and_apply(&state).await {
                        return;
                    }
                    // Failed: retry at the next capture transition (or on
                    // the next boot).
                }
                TuningAction::Wait(_) => {}
            }
            if input.changed().await.is_err() {
                return;
            }
        }
    })
}

/// Run the isolated check and apply its verdict. Returns whether it
/// completed; on `false` nothing was recorded and a later run retries.
async fn run_and_apply(state: &CoreState) -> bool {
    // Share the check lock with the manual button: two concurrent checks
    // would contend for CPU and skew each other's timings.
    let guard = loop {
        match state.audio_benchmark_lock.clone().try_lock_owned() {
            Ok(guard) => break guard,
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
        }
    };
    let measured = state.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        audio_benchmark::run(
            measured.input_sample_rate_hz(),
            measured.output_sample_rate_hz(),
            measured.eq_gains_db(),
            TUNING_ITERATIONS,
            measured.dsp_quality(),
        )
    })
    .await;
    let outcome = match outcome {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => {
            eprintln!("first-run audio tuning could not run: {error}; it will retry later");
            return false;
        }
        Err(error) => {
            eprintln!("first-run audio tuning stopped unexpectedly: {error}; it will retry later");
            return false;
        }
    };
    match state.record_first_run_tuning(&outcome).await {
        Ok(applied) => {
            let BenchmarkOutcome {
                report,
                recommended_quality,
                ..
            } = &outcome;
            eprintln!(
                "first-run audio tuning complete: capture buffer {} ms, DSP quality {:?} \
                 (applied buffer: {}, applied quality: {})",
                report.recommended_buffer_ms,
                recommended_quality,
                applied.buffer,
                applied.quality
            );
            true
        }
        Err(error) => {
            eprintln!("first-run audio tuning could not be applied: {error}; it will retry later");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending() -> SavedSettings {
        SavedSettings::default()
    }

    fn tuned(settings: &mut SavedSettings) {
        settings.recommended_dsp_quality = Some(DspQuality::Economy);
    }

    #[test]
    fn first_use_triggers_tuning_once_capture_is_live() {
        assert_eq!(
            decide(&pending(), true, false, true),
            TuningAction::Run
        );
    }

    #[test]
    fn tuning_waits_for_a_live_capture_and_an_unpaused_service() {
        assert_eq!(
            decide(&pending(), true, false, false),
            TuningAction::Wait("no capture is live yet")
        );
        assert_eq!(
            decide(&pending(), false, false, true),
            TuningAction::Wait("the service is paused")
        );
    }

    #[test]
    fn the_second_boot_does_not_run_tuning_again() {
        let mut settings = pending();
        tuned(&mut settings);
        assert_eq!(
            decide(&settings, true, false, true),
            TuningAction::Never("a check has already completed on this machine")
        );
    }

    #[test]
    fn mock_mode_skips_cleanly_and_leaves_tuning_pending() {
        assert_eq!(
            decide(&pending(), true, true, true),
            TuningAction::Never("mock mode has no audio hardware to tune")
        );
        // Nothing is recorded on a skip: `recommended_dsp_quality` stays
        // `None`, so a real machine retries later.
        assert_eq!(pending().recommended_dsp_quality, None);
    }

    #[test]
    fn the_verdict_applies_only_to_untouched_settings() {
        let mut settings = pending();
        let applied = apply_verdict(&mut settings, 100, DspQuality::Balanced, 1234);
        assert_eq!(
            applied,
            AppliedTuning {
                buffer: true,
                quality: true
            }
        );
        assert_eq!(settings.capture_buffer_ms, 100);
        assert_eq!(settings.dsp_quality, Some(DspQuality::Balanced));
        assert_eq!(settings.recommended_dsp_quality, Some(DspQuality::Balanced));
        assert_eq!(
            settings.first_run_tuning,
            Some(FirstRunTuning {
                ran_at_unix_s: 1234,
                recommended_buffer_ms: 100,
                applied_buffer: true,
                applied_quality: true,
            })
        );
    }

    #[test]
    fn a_users_choice_is_never_overwritten() {
        let mut settings = pending();
        settings.capture_buffer_ms = 200;
        settings.dsp_quality = Some(DspQuality::High);
        let applied = apply_verdict(&mut settings, 100, DspQuality::Balanced, 1234);
        assert_eq!(applied, AppliedTuning::default());
        assert_eq!(settings.capture_buffer_ms, 200);
        assert_eq!(settings.dsp_quality, Some(DspQuality::High));
        // The verdict is still recorded so the user can see it.
        assert_eq!(settings.recommended_dsp_quality, Some(DspQuality::Balanced));
        assert_eq!(
            settings.first_run_tuning,
            Some(FirstRunTuning {
                ran_at_unix_s: 1234,
                recommended_buffer_ms: 100,
                applied_buffer: false,
                applied_quality: false,
            })
        );
        // And tuning never runs again on this machine.
        assert_eq!(
            decide(&settings, true, false, true),
            TuningAction::Never("a check has already completed on this machine")
        );
    }

    #[test]
    fn a_partially_touched_machine_gets_only_the_untouched_setting() {
        let mut settings = pending();
        settings.dsp_quality = Some(DspQuality::High);
        let applied = apply_verdict(&mut settings, 100, DspQuality::Balanced, 1234);
        assert_eq!(
            applied,
            AppliedTuning {
                buffer: true,
                quality: false
            }
        );
        assert_eq!(settings.capture_buffer_ms, 100);
        assert_eq!(settings.dsp_quality, Some(DspQuality::High));
    }

    #[test]
    fn a_failed_check_retries_later() {
        // `apply_verdict` is the only thing that records a check; a failed
        // run never calls it, so the signal stays `None` and the decision
        // is `Run` again at the next opportunity.
        let settings = pending();
        assert_eq!(settings.recommended_dsp_quality, None);
        assert_eq!(settings.first_run_tuning, None);
        assert_eq!(decide(&settings, true, false, true), TuningAction::Run);
    }
}
