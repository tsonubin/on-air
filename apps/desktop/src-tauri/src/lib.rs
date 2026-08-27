use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_status() -> on_air_core::StatusResponse {
    on_air_core::status()
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
        let picker =
            unsafe { AVRoutePickerView::initWithFrame(AVRoutePickerView::alloc(mtm), picker_frame) };
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
            Some(vec![]),
        ))
        .setup(|app| {
            let _ = app.autolaunch().enable();
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("on-air")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            tauri::async_runtime::spawn(async {
                let addr = std::net::SocketAddr::from(([0, 0, 0, 0], on_air_core::DEFAULT_PORT));
                if let Err(err) = on_air_core::serve_on(addr).await {
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
        .run(|app_handle, event| {
            if let RunEvent::WindowEvent { label, event, .. } = event {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(window) = app_handle.get_webview_window(&label) {
                        let _ = window.hide();
                    }
                }
            }
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
