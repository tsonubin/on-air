use crate::dsp::{eq::EQ_GAIN_RANGE_DB, rates, DspQuality};
use crate::session::ActiveOutput;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(200);

/// One output's saved latency trim (`settings.json`, `sync_trims`): the
/// user's manual calibration of that output's playout latency, in
/// milliseconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSyncTrim {
    pub transport: String,
    pub device_id: String,
    pub trim_ms: i32,
}

/// What the automatic first-use tuning measured and did on this machine
/// (`settings.json`, `first_run_tuning`). Persisted so the user can see
/// that tuning ran and what it chose, and override it later.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FirstRunTuning {
    /// Unix seconds when the automatic check ran.
    pub ran_at_unix_s: u64,
    /// The check's capture-buffer verdict, in milliseconds. This is what
    /// was applied if [`FirstRunTuning::applied_buffer`].
    pub recommended_buffer_ms: u32,
    /// Whether the buffer verdict was applied: it is applied only while
    /// `capture_buffer_ms` is untouched (`0`).
    pub applied_buffer: bool,
    /// Whether the quality verdict was applied: it is applied only while
    /// `dsp_quality` is untouched (`None`). The verdict itself is
    /// `recommended_dsp_quality`.
    pub applied_quality: bool,
}

/// One output as a member of a saved sync group (`settings.json`,
/// `sync_groups`). Groups are configuration: after a restart they come back
/// created and stopped, never playing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSyncGroupMember {
    pub transport: String,
    pub device_id: String,
    /// Display name as last seen; discovery may refresh it later.
    #[serde(default)]
    pub device_name: String,
}

/// A saved named sync group (`settings.json`, `sync_groups`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSyncGroup {
    pub name: String,
    #[serde(default)]
    pub members: Vec<SavedSyncGroupMember>,
}

/// A stored DSP profile the wire enum no longer knows reads as "unset":
/// the enum's equivalent of the [`SavedSettings::normalized`] membership
/// check on `capture_buffer_ms`, so a settings file written by a version
/// with other profiles still loads.
fn de_opt_quality<'de, D>(deserializer: D) -> Result<Option<DspQuality>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(raw).ok().flatten())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSettings {
    pub active_input: Option<String>,
    pub active_output: Option<ActiveOutput>,
    pub volume: u8,
    pub eq_gains_db: [f32; 5],
    pub input_sample_rate_hz: u32,
    pub output_sample_rate_hz: u32,
    pub service_enabled: bool,
    pub capture_buffer_ms: u32,
    /// The user's explicit DSP quality choice (`PUT /api/audio/quality`).
    /// `None` = never chosen: the default profile is in effect until the
    /// automatic first-run tuning applies its recommendation. A user's
    /// choice is never overwritten.
    #[serde(deserialize_with = "de_opt_quality")]
    pub dsp_quality: Option<DspQuality>,
    /// The most recent audio check's quality verdict (automatic or the
    /// manual button). `None` = no check has ever completed on this
    /// machine — the trigger for automatic first-run tuning.
    #[serde(deserialize_with = "de_opt_quality")]
    pub recommended_dsp_quality: Option<DspQuality>,
    /// What the automatic first-use tuning measured and applied; `None`
    /// until it has completed (or when it never applied anything).
    pub first_run_tuning: Option<FirstRunTuning>,
    pub sync_trims: Vec<SavedSyncTrim>,
    pub sync_groups: Vec<SavedSyncGroup>,
}

impl Default for SavedSettings {
    fn default() -> Self {
        Self {
            active_input: None,
            active_output: None,
            volume: 50,
            eq_gains_db: [0.0; 5],
            input_sample_rate_hz: crate::state::TARGET_SAMPLE_RATE_DEFAULT_HZ,
            output_sample_rate_hz: crate::state::TARGET_SAMPLE_RATE_DEFAULT_HZ,
            service_enabled: true,
            capture_buffer_ms: 0,
            dsp_quality: None,
            recommended_dsp_quality: None,
            first_run_tuning: None,
            sync_groups: Vec::new(),
            sync_trims: Vec::new(),
        }
    }
}

impl SavedSettings {
    fn normalized(mut self) -> Self {
        if !crate::audio_benchmark::BUFFER_OPTIONS_MS.contains(&self.capture_buffer_ms) {
            self.capture_buffer_ms = 0;
        }
        self.active_input = self.active_input.filter(|name| !name.trim().is_empty());
        self.active_output = self.active_output.filter(|output| {
            matches!(output.transport.as_str(), "sonos" | "airplay" | "bluetooth")
                && !output.device_id.trim().is_empty()
        });
        self.volume = self.volume.min(100);
        self.eq_gains_db = self.eq_gains_db.map(|gain| {
            if gain.is_finite() {
                gain.clamp(EQ_GAIN_RANGE_DB.0, EQ_GAIN_RANGE_DB.1)
            } else {
                0.0
            }
        });
        if !rates::INPUT_RATES_HZ.contains(&self.input_sample_rate_hz) {
            self.input_sample_rate_hz = crate::state::TARGET_SAMPLE_RATE_DEFAULT_HZ;
        }
        if !rates::STANDARD_RATES_HZ.contains(&self.output_sample_rate_hz) {
            self.output_sample_rate_hz = crate::state::TARGET_SAMPLE_RATE_DEFAULT_HZ;
        }
        self.sync_trims.retain_mut(|trim| {
            trim.trim_ms = trim.trim_ms.clamp(
                -crate::sync::latency::TRIM_LIMIT_MS,
                crate::sync::latency::TRIM_LIMIT_MS,
            );
            !trim.transport.trim().is_empty() && !trim.device_id.trim().is_empty()
        });
        self.sync_groups.retain_mut(|group| {
            group.name = group.name.trim().chars().take(64).collect();
            group.members.retain(|member| {
                !member.transport.trim().is_empty() && !member.device_id.trim().is_empty()
            });
            !group.name.is_empty()
        });
        self
    }
}

pub(crate) fn load_file(path: &Path) -> io::Result<SavedSettings> {
    let bytes = fs::read(path)?;
    serde_json::from_slice::<SavedSettings>(&bytes)
        .map(SavedSettings::normalized)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_file(path: &Path, settings: &SavedSettings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        #[cfg(target_os = "windows")]
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
            ) =>
        {
            fs::remove_file(path)?;
            fs::rename(temporary, path)
        }
        Err(error) => Err(error),
    }
}

#[derive(Clone)]
pub(crate) struct SettingsPersistence {
    sender: mpsc::SyncSender<SettingsCommand>,
    pending: Arc<Mutex<Option<SavedSettings>>>,
}

enum SettingsCommand {
    SavePending,
    Flush(SavedSettings, mpsc::SyncSender<io::Result<()>>),
}

impl SettingsPersistence {
    pub(crate) fn new(path: PathBuf) -> Self {
        // One wake-up is enough: the mutex always holds the newest snapshot.
        // This bounds memory even if an authenticated client floods updates.
        let (sender, receiver) = mpsc::sync_channel::<SettingsCommand>(1);
        let pending = Arc::new(Mutex::new(None));
        let thread_pending = pending.clone();
        std::thread::Builder::new()
            .name("on-air-settings".into())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        SettingsCommand::SavePending => loop {
                            match receiver.recv_timeout(SAVE_DEBOUNCE) {
                                Ok(SettingsCommand::SavePending) => {}
                                Ok(SettingsCommand::Flush(newest, completion)) => {
                                    thread_pending.lock().take();
                                    let result = write_file(&path, &newest);
                                    if let Err(error) = result.as_ref() {
                                        eprintln!("could not save on-air settings: {error}");
                                    }
                                    let _ = completion.send(result);
                                    break;
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {
                                    if let Some(settings) = thread_pending.lock().take() {
                                        if let Err(error) = write_file(&path, &settings) {
                                            eprintln!("could not save on-air settings: {error}");
                                        }
                                    }
                                    break;
                                }
                                Err(mpsc::RecvTimeoutError::Disconnected) => {
                                    if let Some(settings) = thread_pending.lock().take() {
                                        if let Err(error) = write_file(&path, &settings) {
                                            eprintln!("could not save on-air settings: {error}");
                                        }
                                    }
                                    return;
                                }
                            }
                        },
                        SettingsCommand::Flush(settings, completion) => {
                            thread_pending.lock().take();
                            let result = write_file(&path, &settings);
                            if let Err(error) = result.as_ref() {
                                eprintln!("could not save on-air settings: {error}");
                            }
                            let _ = completion.send(result);
                        }
                    }
                }
            })
            .expect("could not start settings writer");
        Self { sender, pending }
    }

    pub(crate) fn queue(&self, settings: SavedSettings) {
        *self.pending.lock() = Some(settings);
        match self.sender.try_send(SettingsCommand::SavePending) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {
                eprintln!("could not queue on-air settings: writer stopped");
            }
        }
    }

    pub(crate) fn save_now(&self, settings: &SavedSettings) -> io::Result<()> {
        let (completion, result) = mpsc::sync_channel(0);
        self.sender
            .send(SettingsCommand::Flush(settings.clone(), completion))
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?;
        result
            .recv()
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("on-air-{name}-{}-{nonce}.json", std::process::id()))
    }

    #[test]
    fn settings_round_trip() {
        let path = test_path("round-trip");
        let expected = SavedSettings {
            active_input: Some("System monitor".into()),
            active_output: Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:living-room".into(),
                device_name: "Living Room".into(),
            }),
            volume: 37,
            eq_gains_db: [1.0, 2.0, 3.0, 4.0, 5.0],
            input_sample_rate_hz: 48_000,
            output_sample_rate_hz: 44_100,
            service_enabled: false,
            capture_buffer_ms: 0,
            dsp_quality: Some(DspQuality::High),
            recommended_dsp_quality: Some(DspQuality::Balanced),
            first_run_tuning: Some(FirstRunTuning {
                ran_at_unix_s: 1_760_000_000,
                recommended_buffer_ms: 100,
                applied_buffer: true,
                applied_quality: false,
            }),
            sync_trims: vec![SavedSyncTrim {
                transport: "sonos".into(),
                device_id: "uuid:living-room".into(),
                trim_ms: -250,
            }],
            sync_groups: vec![SavedSyncGroup {
                name: "Downstairs".into(),
                members: vec![SavedSyncGroupMember {
                    transport: "sonos".into(),
                    device_id: "uuid:living-room".into(),
                    device_name: "Living Room".into(),
                }],
            }],
        };
        write_file(&path, &expected).unwrap();
        assert_eq!(load_file(&path).unwrap(), expected);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_settings_file_from_an_older_version_loads_with_tuning_pending() {
        let path = test_path("older-version");
        // No `dsp_quality` / `recommended_dsp_quality` / `first_run_tuning`
        // keys — a pre-tuning settings file.
        fs::write(
            &path,
            br#"{"active_input":"System monitor","volume":40,"capture_buffer_ms":50}"#,
        )
        .unwrap();
        let loaded = load_file(&path).unwrap();
        assert_eq!(loaded.volume, 40);
        assert_eq!(loaded.capture_buffer_ms, 50);
        assert_eq!(loaded.dsp_quality, None);
        assert_eq!(loaded.recommended_dsp_quality, None);
        assert_eq!(loaded.first_run_tuning, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_profile_the_wire_enum_no_longer_knows_reads_as_unset() {
        let path = test_path("unknown-profile");
        fs::write(
            &path,
            br#"{"dsp_quality":"turbo","recommended_dsp_quality":"balanced"}"#,
        )
        .unwrap();
        let loaded = load_file(&path).unwrap();
        assert_eq!(loaded.dsp_quality, None);
        assert_eq!(loaded.recommended_dsp_quality, Some(DspQuality::Balanced));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_saved_values_are_made_safe() {
        let path = test_path("normalize");
        fs::write(
            &path,
            br#"{
                "active_input":"",
                "active_output":{"transport":"unknown","device_id":"","device_name":"x"},
                "volume":255,
                "eq_gains_db":[99.0,-99.0,0.0,1.0,2.0],
                "input_sample_rate_hz":22050,
                "output_sample_rate_hz":12345,
                "service_enabled":true,
                "capture_buffer_ms":7,
                "dsp_quality":"ultra",
                "recommended_dsp_quality":"ultra"
            }"#,
        )
        .unwrap();
        let loaded = load_file(&path).unwrap();
        assert_eq!(loaded.active_input, None);
        assert_eq!(loaded.active_output, None);
        assert_eq!(loaded.volume, 100);
        assert_eq!(loaded.eq_gains_db, [12.0, -12.0, 0.0, 1.0, 2.0]);
        assert_eq!(loaded.input_sample_rate_hz, 44_100);
        assert_eq!(loaded.output_sample_rate_hz, 44_100);
        assert_eq!(loaded.capture_buffer_ms, 0);
        // An unknown profile reads as "unset" — the safe default is in
        // effect — rather than failing the whole file.
        assert_eq!(loaded.dsp_quality, None);
        assert_eq!(loaded.recommended_dsp_quality, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn synchronous_flush_cannot_be_overwritten_by_an_older_debounced_save() {
        let path = test_path("ordered-flush");
        let persistence = SettingsPersistence::new(path.clone());
        let old = SavedSettings {
            volume: 10,
            ..SavedSettings::default()
        };
        let newest = SavedSettings {
            volume: 73,
            ..SavedSettings::default()
        };

        persistence.queue(old);
        persistence.save_now(&newest).unwrap();
        std::thread::sleep(SAVE_DEBOUNCE + Duration::from_millis(20));

        assert_eq!(load_file(&path).unwrap().volume, 73);
        drop(persistence);
        let _ = fs::remove_file(path);
    }
}
