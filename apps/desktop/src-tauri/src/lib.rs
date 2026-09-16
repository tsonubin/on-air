use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

struct ShutdownStarted(AtomicBool);

struct KeepAwake {
    enabled: AtomicBool,
    child: Mutex<Option<std::process::Child>>,
    #[cfg(target_os = "windows")]
    stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    #[cfg(target_os = "windows")]
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl KeepAwake {
    fn start(enabled: bool) -> Self {
        let keep_awake = KeepAwake {
            enabled: AtomicBool::new(false),
            child: Mutex::new(None),
            #[cfg(target_os = "windows")]
            stop: Mutex::new(None),
            #[cfg(target_os = "windows")]
            thread: Mutex::new(None),
        };
        keep_awake.set_enabled(enabled);
        keep_awake
    }

    fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::AcqRel) == enabled {
            return;
        }

        if enabled {
            #[cfg(target_os = "macos")]
            let child = std::process::Command::new("/usr/bin/caffeinate")
                // Keep system/idle sleep disabled while allowing display sleep.
                .args(["-i", "-s", "-w", &std::process::id().to_string()])
                .spawn()
                .map_err(|error| eprintln!("could not create macOS sleep assertion: {error}"))
                .ok();

            #[cfg(target_os = "linux")]
            let child = std::process::Command::new("systemd-inhibit")
                .args([
                    "--what=sleep:idle",
                    "--who=on-air",
                    "--why=Keep the LAN audio service discoverable",
                    "--mode=block",
                    "sleep",
                    "infinity",
                ])
                .spawn()
                .map_err(|error| eprintln!("could not create Linux sleep inhibitor: {error}"))
                .ok();

            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let child = None;

            *self.child.lock().unwrap() = child;

            #[cfg(target_os = "windows")]
            {
                let (stop_tx, stop_rx) = std::sync::mpsc::channel();
                let thread = std::thread::spawn(move || {
                    const ES_CONTINUOUS: u32 = 0x8000_0000;
                    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
                    #[link(name = "kernel32")]
                    unsafe extern "system" {
                        fn SetThreadExecutionState(flags: u32) -> u32;
                    }
                    unsafe {
                        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
                    }
                    let _ = stop_rx.recv();
                    unsafe {
                        SetThreadExecutionState(ES_CONTINUOUS);
                    }
                });
                *self.stop.lock().unwrap() = Some(stop_tx);
                *self.thread.lock().unwrap() = Some(thread);
            }
            return;
        }

        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(stop) = self.stop.lock().unwrap().take() {
                let _ = stop.send(());
            }
            if let Some(thread) = self.thread.lock().unwrap().take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.set_enabled(false);
    }
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_status(
    state: tauri::State<'_, on_air_core::state::CoreState>,
) -> on_air_core::StatusResponse {
    on_air_core::status_with_service(state.service_enabled.load(Ordering::Acquire))
}

#[tauri::command]
fn autostart_enabled(app: tauri::AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn open_airplay_picker(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = tx.send(macos_airplay::present_route_picker());
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok("owntone-list".into())
    }
}

#[cfg(target_os = "macos")]
mod macos_airplay;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            if let Err(error) = app.autolaunch().enable() {
                eprintln!("could not enable on-air login startup: {error}");
            }
            let state = if on_air_core::mock_mode_enabled() {
                tauri::async_runtime::block_on(on_air_core::state::CoreState::new_mock())
            } else {
                let settings_path = app.path().app_config_dir()?.join("settings.json");
                on_air_core::state::CoreState::new_persistent(settings_path)
            };
            let service_enabled = state.service_enabled.load(Ordering::Acquire);
            app.manage(state.clone());
            app.manage(ShutdownStarted(AtomicBool::new(false)));
            app.manage(KeepAwake::start(service_enabled));

            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let service = MenuItem::with_id(
                app,
                "service",
                if service_enabled {
                    "Turn Service Off"
                } else {
                    "Turn Service On"
                },
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&open, &service, &quit])?;
            let service_menu = service.clone();
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("on-air")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "service" => {
                        let state = app.state::<on_air_core::state::CoreState>();
                        let enabled = !state.service_enabled.load(Ordering::Acquire);
                        state.set_service_enabled(enabled);
                        let _ = state
                            .ws_tx
                            .send(on_air_core::api::ws::WsEvent::ServiceStateChanged { enabled });
                        app.state::<KeepAwake>().set_enabled(enabled);
                        let _ = service_menu.set_text(if enabled {
                            "Turn Service Off"
                        } else {
                            "Turn Service On"
                        });
                        if !enabled {
                            let _ = service_menu.set_enabled(false);
                            let service_menu = service_menu.clone();
                            let state = state.inner().clone();
                            tauri::async_runtime::spawn(async move {
                                state.shutdown().await;
                                let _ = service_menu.set_enabled(true);
                            });
                        } else {
                            state.spawn_saved_session_restore();
                        }
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _tray = tray.build(app)?;
            if std::env::args_os().any(|arg| arg == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], on_air_core::DEFAULT_PORT));
            let listener = match std::net::TcpListener::bind(addr) {
                Ok(listener) => listener,
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    eprintln!(
                        "on-air is already running (port {} is in use)",
                        on_air_core::DEFAULT_PORT
                    );
                    app.handle().exit(0);
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            listener.set_nonblocking(true)?;
            tauri::async_runtime::spawn(async move {
                if let Err(err) = on_air_core::serve_with_state_std(listener, state).await {
                    eprintln!("on-air-core HTTP server failed: {err}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_status,
            autostart_enabled,
            open_airplay_picker
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            RunEvent::ExitRequested { api, .. } => {
                let Some(shutdown) = app_handle.try_state::<ShutdownStarted>() else {
                    return;
                };
                if !shutdown.0.swap(true, Ordering::AcqRel) {
                    api.prevent_exit();
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        handle
                            .state::<on_air_core::state::CoreState>()
                            .shutdown()
                            .await;
                        handle.exit(0);
                    });
                }
            }
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                api.prevent_close();
                if let Some(window) = app_handle.get_webview_window(&label) {
                    let _ = window.hide();
                }
            }
            _ => {}
        });
}
