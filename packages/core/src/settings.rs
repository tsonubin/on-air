use crate::dsp::{eq::EQ_GAIN_RANGE_DB, rates};
use crate::session::ActiveOutput;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(200);

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
        }
    }
}

impl SavedSettings {
    fn normalized(mut self) -> Self {
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
        };
        write_file(&path, &expected).unwrap();
        assert_eq!(load_file(&path).unwrap(), expected);
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
                "service_enabled":true
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
