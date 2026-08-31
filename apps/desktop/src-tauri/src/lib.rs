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
    fn start() -> Self {
        let keep_awake = KeepAwake {
            enabled: AtomicBool::new(false),
            child: Mutex::new(None),
            #[cfg(target_os = "windows")]
            stop: Mutex::new(None),
            #[cfg(target_os = "windows")]
            thread: Mutex::new(None),
        };
        keep_awake.set_enabled(true);
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
mod macos_airplay {
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
    use objc2_av_kit::AVRoutePickerView;
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use std::cell::RefCell;

    thread_local! {
        static LIVE_WINDOWS: RefCell<Vec<Retained<NSWindow>>> = const { RefCell::new(Vec::new()) };
    }

    pub fn live_window_count() -> usize {
        LIVE_WINDOWS.with(|w| w.borrow().len())
    }

    pub fn picker_is_installed_in_a_window() -> bool {
        LIVE_WINDOWS.with(|w| {
            w.borrow().last().is_some_and(|window| {
                window
                    .contentView()
                    .is_some_and(|view| !view.subviews().is_empty())
            })
        })
    }

    pub fn present_route_picker() -> Result<String, String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "AVRoutePickerView must be created on the main thread".to_string())?;
        let frame = NSRect::new(NSPoint::new(200.0, 200.0), NSSize::new(240.0, 80.0));
        let picker_frame = NSRect::new(NSPoint::new(80.0, 20.0), NSSize::new(80.0, 40.0));
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setTitle(&NSString::from_str("AirPlay"));
        let picker = unsafe {
            AVRoutePickerView::initWithFrame(AVRoutePickerView::alloc(mtm), picker_frame)
        };
        let content = window
            .contentView()
            .ok_or_else(|| "window has no content view".to_string())?;
        content.addSubview(&picker);
        window.makeKeyAndOrderFront(None);
        if content.subviews().is_empty() {
            return Err("AVRoutePickerView was not added to the window".into());
        }
        LIVE_WINDOWS.with(|w| w.borrow_mut().push(window));
        Ok("avroute-picker".into())
    }
}

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
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let service =
                MenuItem::with_id(app, "service", "Turn Service Off", true, None::<&str>)?;
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
                        state.service_enabled.store(enabled, Ordering::Release);
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
                            let state = state.inner().clone();
                            tauri::async_runtime::spawn(async move {
                                state.shutdown().await;
                            });
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
            let state = if on_air_core::mock_mode_enabled() {
                tauri::async_runtime::block_on(on_air_core::state::CoreState::new_mock())
            } else {
                on_air_core::state::CoreState::new()
            };
            app.manage(state.clone());
            app.manage(ShutdownStarted(AtomicBool::new(false)));
            app.manage(KeepAwake::start());
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn airplay_picker_is_attached_to_a_live_window() {
        let before = super::macos_airplay::live_window_count();
        match super::macos_airplay::present_route_picker() {
            Ok(label) => {
                assert_eq!(label, "avroute-picker");
                assert!(super::macos_airplay::live_window_count() > before);
                assert!(super::macos_airplay::picker_is_installed_in_a_window());
            }
            Err(err) if err.contains("main thread") => {
                // cargo test worker threads are not the AppKit main thread.
                eprintln!("skipping picker UI attach: {err}");
            }
            Err(err) => panic!("{err}"),
        }
    }
}
