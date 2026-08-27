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
fn open_airplay_picker() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        macos_airplay::present_route_picker()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok("owntone-list".into())
    }
}

#[cfg(target_os = "macos")]
mod macos_airplay {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    pub fn present_route_picker() -> Result<String, String> {
        unsafe {
            let avkit = class!(AVRoutePickerView);
            let view: Retained<AnyObject> = msg_send![avkit, new];
            let _shown: () = msg_send![&view, setActiveTintColor: std::ptr::null::<AnyObject>()];
            let _ = view;
        }
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
