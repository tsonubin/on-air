//! Sleep inhibitor held while the service is on.
//!
//! A LAN audio service is useless on a sleeping machine, so the desktop app
//! holds a platform sleep inhibitor for as long as the service is enabled:
//! `caffeinate -w <pid>` on macOS, `systemd-inhibit … sleep infinity` on Linux
//! and `SetThreadExecutionState` on Windows. The OS-facing part is behind
//! [`InhibitorSpawner`] so the enable/disable/exit sequencing can be tested
//! without touching the OS, and releasing never blocks the calling thread:
//! the tray handler and the exit path only signal the helper and move on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// How the machine is kept awake on one platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InhibitMethod {
    /// Keep a helper process alive for as long as the inhibitor is held.
    #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
    Process { program: String, args: Vec<String> },
    /// Windows `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`
    /// on a dedicated thread.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    ExecutionState,
}

/// The inhibitor for the platform this binary was built for, if it has one.
pub fn platform_inhibit_method() -> Option<InhibitMethod> {
    #[cfg(target_os = "macos")]
    {
        Some(InhibitMethod::Process {
            program: "/usr/bin/caffeinate".into(),
            // Keep system and idle sleep disabled while allowing display sleep;
            // `-w` makes caffeinate exit on its own if this process dies.
            args: vec![
                "-i".into(),
                "-s".into(),
                "-w".into(),
                std::process::id().to_string(),
            ],
        })
    }
    #[cfg(target_os = "linux")]
    {
        Some(InhibitMethod::Process {
            program: "systemd-inhibit".into(),
            args: vec![
                "--what=sleep:idle".into(),
                "--who=on-air".into(),
                "--why=Keep the LAN audio service discoverable".into(),
                "--mode=block".into(),
                "sleep".into(),
                "infinity".into(),
            ],
        })
    }
    #[cfg(target_os = "windows")]
    {
        Some(InhibitMethod::ExecutionState)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// A held inhibitor. Dropping it without [`Inhibitor::release`] leaks it.
pub trait Inhibitor: Send {
    /// Lets the machine sleep again. Must return promptly: the caller may be
    /// the tray event thread or the exit path.
    fn release(self: Box<Self>);
}

/// Starts inhibitors. The production implementation talks to the OS; tests
/// record what would have been started.
pub trait InhibitorSpawner: Send + Sync {
    fn spawn(&self, method: &InhibitMethod) -> Option<Box<dyn Inhibitor>>;
}

/// Spawns the real helper process or execution-state thread.
pub struct SystemSpawner;

impl InhibitorSpawner for SystemSpawner {
    fn spawn(&self, method: &InhibitMethod) -> Option<Box<dyn Inhibitor>> {
        match method {
            InhibitMethod::Process { program, args } => std::process::Command::new(program)
                .args(args)
                .spawn()
                .map_err(|error| eprintln!("could not start sleep inhibitor {program}: {error}"))
                .ok()
                .map(|child| Box::new(ProcessInhibitor(child)) as Box<dyn Inhibitor>),
            InhibitMethod::ExecutionState => {
                #[cfg(target_os = "windows")]
                {
                    Some(Box::new(ExecutionStateInhibitor::start()) as Box<dyn Inhibitor>)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    eprintln!("SetThreadExecutionState is only available on Windows");
                    None
                }
            }
        }
    }
}

struct ProcessInhibitor(std::process::Child);

impl Inhibitor for ProcessInhibitor {
    fn release(self: Box<Self>) {
        let mut child = self.0;
        let _ = child.kill();
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        // Reap off the caller's thread. If the process exits before this
        // thread runs, init adopts the helper, which is already dead.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(target_os = "windows")]
struct ExecutionStateInhibitor {
    stop: std::sync::mpsc::Sender<()>,
}

#[cfg(target_os = "windows")]
impl ExecutionStateInhibitor {
    fn start() -> Self {
        let (stop, stop_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            const ES_CONTINUOUS: u32 = 0x8000_0000;
            const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn SetThreadExecutionState(flags: u32) -> u32;
            }
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            }
            // Returns on a stop signal or when the sender is dropped.
            let _ = stop_rx.recv();
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        });
        Self { stop }
    }
}

#[cfg(target_os = "windows")]
impl Inhibitor for ExecutionStateInhibitor {
    fn release(self: Box<Self>) {
        // The execution state is per thread, so the thread resets it and
        // exits on its own; nothing joins it.
        let _ = self.stop.send(());
    }
}

/// Holds the platform inhibitor while enabled.
pub struct KeepAwake {
    enabled: AtomicBool,
    method: Option<InhibitMethod>,
    spawner: Box<dyn InhibitorSpawner>,
    active: Mutex<Option<Box<dyn Inhibitor>>>,
}

impl KeepAwake {
    /// The real inhibitor for this platform, held immediately when `enabled`.
    pub fn system(enabled: bool) -> Self {
        Self::with_spawner(platform_inhibit_method(), Box::new(SystemSpawner), enabled)
    }

    pub fn with_spawner(
        method: Option<InhibitMethod>,
        spawner: Box<dyn InhibitorSpawner>,
        enabled: bool,
    ) -> Self {
        let keep_awake = Self {
            enabled: AtomicBool::new(false),
            method,
            spawner,
            active: Mutex::new(None),
        };
        keep_awake.set_enabled(enabled);
        keep_awake
    }

    /// Test probe; production code reads the core's service flag instead.
    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Idempotent; releasing never blocks the caller.
    pub fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::AcqRel) == enabled {
            return;
        }
        let previous = if enabled {
            let started = self
                .method
                .as_ref()
                .and_then(|method| self.spawner.spawn(method));
            std::mem::replace(&mut *self.active.lock().unwrap(), started)
        } else {
            self.active.lock().unwrap().take()
        };
        if let Some(previous) = previous {
            previous.release();
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.set_enabled(false);
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Event {
        Started(InhibitMethod),
        Released,
    }

    /// Records what the real spawner would have done.
    #[derive(Clone, Default)]
    pub struct RecordingSpawner {
        pub events: Arc<Mutex<Vec<Event>>>,
    }

    impl RecordingSpawner {
        pub fn events(&self) -> Vec<Event> {
            self.events.lock().unwrap().clone()
        }
    }

    struct RecordedInhibitor(Arc<Mutex<Vec<Event>>>);

    impl Inhibitor for RecordedInhibitor {
        fn release(self: Box<Self>) {
            self.0.lock().unwrap().push(Event::Released);
        }
    }

    impl InhibitorSpawner for RecordingSpawner {
        fn spawn(&self, method: &InhibitMethod) -> Option<Box<dyn Inhibitor>> {
            self.events
                .lock()
                .unwrap()
                .push(Event::Started(method.clone()));
            Some(Box::new(RecordedInhibitor(self.events.clone())))
        }
    }

    pub fn fake_method() -> InhibitMethod {
        InhibitMethod::Process {
            program: "fake-inhibit".into(),
            args: vec!["--forever".into()],
        }
    }

    /// A `KeepAwake` whose inhibitor is recorded instead of started.
    pub fn recording(enabled: bool) -> (KeepAwake, RecordingSpawner) {
        let spawner = RecordingSpawner::default();
        let keep_awake =
            KeepAwake::with_spawner(Some(fake_method()), Box::new(spawner.clone()), enabled);
        (keep_awake, spawner)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{fake_method, recording, Event};
    use super::*;

    #[test]
    fn enable_disable_and_drop_issue_one_inhibitor_each_way() {
        let (keep_awake, spawner) = recording(true);
        assert!(keep_awake.is_enabled());
        assert_eq!(spawner.events(), vec![Event::Started(fake_method())]);

        keep_awake.set_enabled(true);
        assert_eq!(spawner.events().len(), 1, "enabling twice starts nothing");

        keep_awake.set_enabled(false);
        assert!(!keep_awake.is_enabled());
        assert_eq!(
            spawner.events(),
            vec![Event::Started(fake_method()), Event::Released]
        );

        keep_awake.set_enabled(false);
        assert_eq!(
            spawner.events().len(),
            2,
            "disabling twice releases nothing"
        );

        keep_awake.set_enabled(true);
        drop(keep_awake);
        assert_eq!(
            spawner.events(),
            vec![
                Event::Started(fake_method()),
                Event::Released,
                Event::Started(fake_method()),
                Event::Released,
            ]
        );
    }

    #[test]
    fn starting_disabled_touches_nothing() {
        let (keep_awake, spawner) = recording(false);
        assert!(!keep_awake.is_enabled());
        drop(keep_awake);
        assert!(spawner.events().is_empty());
    }

    #[test]
    fn a_platform_without_an_inhibitor_still_tracks_the_flag() {
        let spawner = testing::RecordingSpawner::default();
        let keep_awake = KeepAwake::with_spawner(None, Box::new(spawner.clone()), true);
        assert!(keep_awake.is_enabled());
        keep_awake.set_enabled(false);
        assert!(spawner.events().is_empty());
    }

    #[test]
    fn the_platform_method_matches_the_documented_helper() {
        let method = platform_inhibit_method();
        #[cfg(target_os = "linux")]
        assert!(matches!(
            &method,
            Some(InhibitMethod::Process { program, args })
                if program == "systemd-inhibit"
                    && args.contains(&"--mode=block".to_string())
                    && args.ends_with(&["sleep".to_string(), "infinity".to_string()])
        ));
        #[cfg(target_os = "macos")]
        assert!(matches!(
            &method,
            Some(InhibitMethod::Process { program, args })
                if program == "/usr/bin/caffeinate"
                    && args.ends_with(&["-w".to_string(), std::process::id().to_string()])
        ));
        #[cfg(target_os = "windows")]
        assert_eq!(method, Some(InhibitMethod::ExecutionState));
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        assert_eq!(method, None);
    }

    #[cfg(unix)]
    #[test]
    fn releasing_a_real_helper_does_not_wait_for_it() {
        let method = InhibitMethod::Process {
            program: "sleep".into(),
            args: vec!["30".into()],
        };
        let keep_awake = KeepAwake::with_spawner(Some(method), Box::new(SystemSpawner), true);
        let started = std::time::Instant::now();
        keep_awake.set_enabled(false);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "release must kill the helper and return without waiting out its sleep"
        );
    }
}
