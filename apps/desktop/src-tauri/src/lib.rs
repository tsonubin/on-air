use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
#[cfg(not(target_os = "macos"))]
use tauri_plugin_autostart::ManagerExt;

mod login_item;
mod tray_icon;
mod window_presentation;

use window_presentation::WindowPresentation;

struct AutostartMenu(CheckMenuItem<tauri::Wry>);

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
    app.try_state::<AutostartMenu>()
        .and_then(|menu| menu.0.is_checked().ok())
        .unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    match apply_autostart(&app, enabled) {
        Ok(actual) => {
            if let Some(menu) = app.try_state::<AutostartMenu>() {
                let _ = menu.0.set_checked(actual);
            }
            let _ = app.emit("autostart-changed", actual);
            Ok(actual)
        }
        Err(error) => {
            if let Some(menu) = app.try_state::<AutostartMenu>() {
                let _ = menu.0.set_checked(!enabled);
            }
            Err(error)
        }
    }
}

/// A development binary started with `--hidden` has no dev server at login, so
/// its window cannot load. Drop the legacy login item and do not stay running
/// on the control port.
pub fn exit_if_hidden_dev_launch() -> bool {
    if !tauri::is_dev() || !login_item::launched_hidden(std::env::args_os()) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    if login_item::bundle_from_exe(&exe).is_some() {
        return false;
    }
    if let Some(home) = std::env::var_os("HOME") {
        login_item::remove_legacy_agent(std::path::Path::new(&home));
    }
    eprintln!(
        "on-air was started at login from a development build, which cannot load its window. That login item was removed."
    );
    true
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

fn reveal_main_window(app: &tauri::AppHandle) {
    if let Some(presentation) = app.try_state::<WindowPresentation>() {
        presentation.request_show();
    }
    #[cfg(target_os = "macos")]
    unhide_and_activate();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "macos")]
fn unhide_and_activate() {
    let Some(marker) = objc2::MainThreadMarker::new() else {
        return;
    };
    let application = objc2_app_kit::NSApplication::sharedApplication(marker);
    application.unhide(None);
    // The tray click is a user event, so this activation is allowed to proceed.
    application.activate();
}

fn hide_main_window_if_pending(app: &tauri::AppHandle) {
    let Some(presentation) = app.try_state::<WindowPresentation>() else {
        return;
    };
    if !presentation.take_autohide() {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// `didFinish` runs inside the turn that has not yet committed the webview's
/// first frame. Hiding there leaves the layer empty, and showing the window
/// from the tray later stays blank. A later main-queue turn runs after that
/// commit.
fn schedule_autohide(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    defer_main_turns(app, 1);
    #[cfg(not(target_os = "macos"))]
    {
        std::thread::spawn(move || {
            let queued = app.clone();
            let _ = app.run_on_main_thread(move || hide_main_window_if_pending(&queued));
        });
    }
}

#[cfg(target_os = "macos")]
fn defer_main_turns(app: tauri::AppHandle, turns: u8) {
    struct Job {
        app: tauri::AppHandle,
        turns: u8,
    }

    extern "C" fn work(context: *mut std::ffi::c_void) {
        let job = unsafe { Box::from_raw(context.cast::<Job>()) };
        if job.turns == 0 {
            hide_main_window_if_pending(&job.app);
        } else {
            defer_main_turns(job.app, job.turns - 1);
        }
    }

    extern "C" {
        fn dispatch_get_main_queue() -> *mut std::ffi::c_void;
        fn dispatch_async_f(
            queue: *mut std::ffi::c_void,
            context: *mut std::ffi::c_void,
            work: extern "C" fn(*mut std::ffi::c_void),
        );
    }

    let job = Box::into_raw(Box::new(Job { app, turns }));
    unsafe {
        dispatch_async_f(dispatch_get_main_queue(), job.cast(), work);
    }
}

fn sync_autostart(app: &tauri::App) -> Result<bool, Box<dyn std::error::Error>> {
    let dir = app.path().app_config_dir()?;
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let home = std::path::PathBuf::from(home);
        let plist = login_item::read_plist_kind(&login_item::launch_agent_plist(&home));
        let bundle = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(login_item::bundle_from_exe);
        let plan =
            login_item::startup_plan(login_item::load_preference(&dir), &plist, bundle.as_deref());
        login_item::apply_action(&home, &plan.action)?;
        if let Some(enabled) = plan.persist {
            login_item::save_preference(&dir, enabled)?;
        }
        Ok(plan.checked)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let preference = login_item::load_preference(&dir);
        let enabled = preference.unwrap_or(true);
        let auto = app.autolaunch();
        let result = if enabled {
            auto.enable()
        } else {
            auto.disable()
        };
        result.map_err(|error| std::io::Error::other(error.to_string()))?;
        if preference.is_none() {
            login_item::save_preference(&dir, enabled)?;
        }
        Ok(enabled)
    }
}

fn apply_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let bundle = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(login_item::bundle_from_exe);
        let action = login_item::user_choice(enabled, bundle.as_deref())?;
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        login_item::apply_action(std::path::Path::new(&home), &action)
            .map_err(|error| error.to_string())?;
        let checked = matches!(action, login_item::LoginItemAction::Install { .. });
        login_item::save_preference(&dir, checked).map_err(|error| error.to_string())?;
        Ok(checked)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let auto = app.autolaunch();
        let result = if enabled {
            auto.enable()
        } else {
            auto.disable()
        };
        result.map_err(|error| error.to_string())?;
        login_item::save_preference(&dir, enabled).map_err(|error| error.to_string())?;
        Ok(enabled)
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
        .on_page_load(|webview, payload| {
            if webview.label() != "main"
                || !matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
            {
                return;
            }
            let Some(presentation) = webview.try_state::<WindowPresentation>() else {
                return;
            };
            if !presentation.should_hide_after_load() {
                return;
            }
            schedule_autohide(webview.app_handle().clone());
        })
        .setup(|app| {
            app.manage(WindowPresentation::new(login_item::launched_hidden(
                std::env::args_os(),
            )));
            let autostart_on = match sync_autostart(app) {
                Ok(enabled) => enabled,
                Err(error) => {
                    eprintln!("could not sync on-air login startup: {error}");
                    false
                }
            };
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
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "Open at Login",
                true,
                autostart_on,
                None::<&str>,
            )?;
            app.manage(AutostartMenu(autostart.clone()));
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
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open, &autostart, &service, &separator, &quit])?;
            let service_menu = service.clone();
            let autostart_menu = autostart.clone();
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("on-air")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "open" => reveal_main_window(app),
                    "autostart" => {
                        let want = autostart_menu.is_checked().unwrap_or(false);
                        if let Err(error) = set_autostart(app.clone(), want) {
                            eprintln!("could not update on-air login startup: {error}");
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
            #[cfg(target_os = "macos")]
            {
                let rgba = tray_icon::menu_bar_icon_rgba();
                let image = tauri::image::Image::new(
                    &rgba,
                    tray_icon::TRAY_ICON_PX,
                    tray_icon::TRAY_ICON_PX,
                );
                tray = tray.icon(image).icon_as_template(true);
            }
            #[cfg(not(target_os = "macos"))]
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _tray = tray.build(app)?;

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
            set_autostart,
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
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => reveal_main_window(app_handle),
            _ => {}
        });
}
