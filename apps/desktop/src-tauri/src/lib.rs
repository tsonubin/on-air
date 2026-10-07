//! on-air desktop shell.
//!
//! Embeds the core service in this process, owns the tray menu, the sleep
//! inhibitor, the login item and the window lifecycle. The webview talks to
//! the core over loopback HTTP/WebSocket; the IPC commands below cover only
//! what the page cannot reach that way.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

mod keep_awake;
mod login_item;
#[cfg(target_os = "macos")]
mod macos_airplay;
mod tray_icon;
mod window_presentation;

use keep_awake::KeepAwake;
use on_air_core::api::ws::WsEvent;
use on_air_core::state::CoreState;
use window_presentation::WindowPresentation;

struct AutostartMenu(CheckMenuItem<tauri::Wry>);

struct ServiceMenu(MenuItem<tauri::Wry>);

struct ShutdownStarted(AtomicBool);

/// Whether the saved "open at login" preference is on. This is the
/// preference file, not the tray item, so it is right even before the tray
/// exists and on platforms where the OS owns the login item.
#[tauri::command]
fn autostart_enabled(app: tauri::AppHandle) -> bool {
    app.path()
        .app_config_dir()
        .ok()
        .and_then(|dir| login_item::load_preference(&dir))
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

/// Whether the LAN service is on (the same flag `/api/status` reports).
#[tauri::command]
fn service_enabled(app: tauri::AppHandle) -> bool {
    app.try_state::<CoreState>()
        .is_some_and(|state| state.service_enabled.load(Ordering::Acquire))
}

/// Turns the LAN service on or off, exactly as the tray item does, and
/// returns the resulting state.
#[tauri::command]
fn set_service_enabled(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    apply_service_enabled(&app, enabled)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServiceTransition {
    Unchanged,
    TurnOn,
    TurnOff,
}

fn service_transition(current: bool, requested: bool) -> ServiceTransition {
    match (current, requested) {
        (false, true) => ServiceTransition::TurnOn,
        (true, false) => ServiceTransition::TurnOff,
        _ => ServiceTransition::Unchanged,
    }
}

/// Label of the tray item that toggles the service.
fn service_menu_label(enabled: bool) -> &'static str {
    if enabled {
        "Turn Service Off"
    } else {
        "Turn Service On"
    }
}

/// The core-side effects of a service change, in order: persist the flag,
/// tell WebSocket clients, then hold or release the sleep inhibitor.
fn apply_service_transition(
    state: &CoreState,
    keep_awake: &KeepAwake,
    transition: ServiceTransition,
) {
    let enabled = match transition {
        ServiceTransition::Unchanged => return,
        ServiceTransition::TurnOn => true,
        ServiceTransition::TurnOff => false,
    };
    state.set_service_enabled(enabled);
    let _ = state.ws_tx.send(WsEvent::ServiceStateChanged { enabled });
    keep_awake.set_enabled(enabled);
}

/// Shared by the tray item and the `set_service_enabled` command so both
/// keep the core, the inhibitor, the tray label and the window in step.
fn apply_service_enabled(app: &tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    let state = app
        .try_state::<CoreState>()
        .ok_or("the on-air service is not running in this process")?;
    let keep_awake = app
        .try_state::<KeepAwake>()
        .ok_or("the on-air service is not running in this process")?;
    let transition = service_transition(state.service_enabled.load(Ordering::Acquire), enabled);
    if transition == ServiceTransition::Unchanged {
        return Ok(enabled);
    }
    apply_service_transition(&state, &keep_awake, transition);
    let menu = app.try_state::<ServiceMenu>();
    if let Some(menu) = &menu {
        let _ = menu.0.set_text(service_menu_label(enabled));
    }
    let _ = app.emit("service-changed", enabled);
    match transition {
        ServiceTransition::TurnOff => {
            // Keep the item unclickable until the audio resources are down.
            let menu = menu.map(|menu| menu.0.clone());
            if let Some(menu) = &menu {
                let _ = menu.set_enabled(false);
            }
            let state = state.inner().clone();
            tauri::async_runtime::spawn(async move {
                state.shutdown().await;
                if let Some(menu) = menu {
                    let _ = menu.set_enabled(true);
                }
            });
        }
        ServiceTransition::TurnOn => {
            state.spawn_saved_session_restore();
        }
        ServiceTransition::Unchanged => {}
    }
    Ok(enabled)
}

/// Everything between an exit request and process exit, in order: stop the
/// audio resources, release the sleep inhibitor, then exit. The inhibitor is
/// a child process on macOS and Linux, so skipping this leaves it running.
async fn finish_exit(
    shutdown: impl Future<Output = ()>,
    keep_awake: &KeepAwake,
    exit: impl FnOnce(),
) {
    shutdown.await;
    keep_awake.set_enabled(false);
    exit();
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

/// Runs the pending autohide after `turns` further main-queue turns.
///
/// Why this exists: a login launch (`--hidden`) must start with the window
/// hidden, but the window has to be shown once so WebKit paints its first
/// frame; hiding inside the page-load callback happens before that frame is
/// committed and the window comes back blank from the tray. Hopping over the
/// main queue with libdispatch lands after the commit. `run_on_main_thread`
/// is not enough on macOS because it may run inside the same turn.
///
/// The alternative is to create the window with `visible: false` and show it
/// on `PageLoadEvent::Finished` when the launch is not hidden, which needs no
/// FFI; this hand-rolled binding is kept until that path is verified to paint
/// on the first show.
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

/// Brings the OS login item in line with the saved preference and returns
/// whether the tray item should show it checked. With no saved preference
/// nothing is installed: autostart is opt-in.
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
        let auto = app.autolaunch();
        let os_enabled = auto
            .is_enabled()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let plan = login_item::autostart_plan(login_item::load_preference(&dir), os_enabled);
        if let Some(enable) = plan.apply {
            let result = if enable {
                auto.enable()
            } else {
                auto.disable()
            };
            result.map_err(|error| std::io::Error::other(error.to_string()))?;
        }
        if let Some(enabled) = plan.persist {
            login_item::save_preference(&dir, enabled)?;
        }
        Ok(plan.checked)
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
    // The mock catalog takes an async lock while it is built. Build it here,
    // before the runtime has anything else to do, rather than blocking the
    // main thread inside `setup`. The persistent state needs the app's config
    // directory and is built in `setup`.
    let mock_state = on_air_core::mock_mode_enabled()
        .then(|| tauri::async_runtime::block_on(CoreState::new_mock()));

    let builder = tauri::Builder::default();
    // macOS keeps its login item in `login_item.rs` (a LaunchAgent that opens
    // the bundle in an Aqua session); the plugin's launcher runs a bare
    // executable there, so it is not registered on macOS.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        MacosLauncher::LaunchAgent,
        Some(vec!["--hidden"]),
    ));
    builder
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
        .setup(move |app| {
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
            let state = match mock_state {
                Some(state) => state,
                None => {
                    let settings_path = app.path().app_config_dir()?.join("settings.json");
                    CoreState::new_persistent(settings_path)
                }
            };
            let service_on = state.service_enabled.load(Ordering::Acquire);
            app.manage(state.clone());
            app.manage(ShutdownStarted(AtomicBool::new(false)));
            app.manage(KeepAwake::system(service_on));

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
                service_menu_label(service_on),
                true,
                None::<&str>,
            )?;
            app.manage(ServiceMenu(service.clone()));
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open, &autostart, &service, &separator, &quit])?;
            let autostart_menu = autostart.clone();
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("on-air")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    // `exit` raises `ExitRequested`, where the core is stopped
                    // and the inhibitor released before the process ends.
                    "quit" => app.exit(0),
                    "open" => reveal_main_window(app),
                    "autostart" => {
                        let want = autostart_menu.is_checked().unwrap_or(false);
                        if let Err(error) = set_autostart(app.clone(), want) {
                            eprintln!("could not update on-air login startup: {error}");
                        }
                    }
                    "service" => {
                        let want = !service_enabled(app.clone());
                        if let Err(error) = apply_service_enabled(app, want) {
                            eprintln!("could not change the on-air service state: {error}");
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
                    // A second launch. Without `tauri-plugin-single-instance`
                    // this process has no channel to the running one, so it
                    // cannot reveal that window, and no dialog plugin is a
                    // dependency, so stderr is the only place to say so. The
                    // running instance stays reachable from its tray icon.
                    eprintln!(
                        "on-air is already running: port {} is in use. Use the tray icon of the running instance to open its window.",
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
            autostart_enabled,
            set_autostart,
            service_enabled,
            set_service_enabled,
            open_airplay_picker
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            RunEvent::ExitRequested { api, code, .. } => {
                let Some(shutdown) = app_handle.try_state::<ShutdownStarted>() else {
                    return;
                };
                if !shutdown.0.swap(true, Ordering::AcqRel) {
                    api.prevent_exit();
                    let handle = app_handle.clone();
                    let code = code.unwrap_or(0);
                    tauri::async_runtime::spawn(async move {
                        let core = handle.try_state::<CoreState>().map(|state| state.inner().clone());
                        let stop_core = async move {
                            if let Some(core) = core {
                                core.shutdown().await;
                            }
                        };
                        match handle.try_state::<KeepAwake>() {
                            Some(keep_awake) => {
                                finish_exit(stop_core, &keep_awake, || handle.exit(code)).await
                            }
                            None => {
                                stop_core.await;
                                handle.exit(code);
                            }
                        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use keep_awake::testing::{fake_method, recording, Event};
    use std::sync::{Arc, Mutex};

    #[test]
    fn the_service_item_names_the_action_it_performs() {
        assert_eq!(service_menu_label(true), "Turn Service Off");
        assert_eq!(service_menu_label(false), "Turn Service On");
    }

    #[test]
    fn a_request_for_the_current_state_changes_nothing() {
        assert_eq!(service_transition(true, true), ServiceTransition::Unchanged);
        assert_eq!(
            service_transition(false, false),
            ServiceTransition::Unchanged
        );
        assert_eq!(service_transition(false, true), ServiceTransition::TurnOn);
        assert_eq!(service_transition(true, false), ServiceTransition::TurnOff);
    }

    #[test]
    fn turning_the_service_off_and_on_updates_core_clients_and_inhibitor() {
        let state = CoreState::new();
        let (keep_awake, spawner) = recording(true);
        let mut events = state.ws_tx.subscribe();
        let current = state.service_enabled.load(Ordering::Acquire);

        apply_service_transition(&state, &keep_awake, ServiceTransition::Unchanged);
        assert_eq!(state.service_enabled.load(Ordering::Acquire), current);
        assert!(events.try_recv().is_err(), "no change, no event");
        assert_eq!(spawner.events(), vec![Event::Started(fake_method())]);

        apply_service_transition(&state, &keep_awake, ServiceTransition::TurnOff);
        assert!(!state.service_enabled.load(Ordering::Acquire));
        assert!(matches!(
            events.try_recv(),
            Ok(WsEvent::ServiceStateChanged { enabled: false })
        ));
        assert!(!keep_awake.is_enabled());
        assert_eq!(
            spawner.events(),
            vec![Event::Started(fake_method()), Event::Released]
        );

        apply_service_transition(&state, &keep_awake, ServiceTransition::TurnOn);
        assert!(state.service_enabled.load(Ordering::Acquire));
        assert!(matches!(
            events.try_recv(),
            Ok(WsEvent::ServiceStateChanged { enabled: true })
        ));
        assert!(keep_awake.is_enabled());
    }

    #[test]
    fn exit_stops_the_core_then_releases_the_inhibitor_then_exits() {
        let (keep_awake, spawner) = recording(true);
        let held = &keep_awake;
        let log = Arc::new(Mutex::new(Vec::new()));
        let shutdown_log = log.clone();
        let exit_log = log.clone();
        tauri::async_runtime::block_on(finish_exit(
            async move {
                shutdown_log.lock().unwrap().push("shutdown");
                assert_eq!(
                    spawner.events(),
                    vec![Event::Started(fake_method())],
                    "inhibitor is still held while audio stops"
                );
            },
            held,
            move || {
                assert!(!held.is_enabled(), "inhibitor is released before exit");
                exit_log.lock().unwrap().push("exit");
            },
        ));
        assert_eq!(*log.lock().unwrap(), vec!["shutdown", "exit"]);
    }
}
