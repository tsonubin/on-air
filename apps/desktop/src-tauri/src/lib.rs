// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_status() -> on_air_core::StatusResponse {
    on_air_core::status()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            tauri::async_runtime::spawn(async {
                let addr = std::net::SocketAddr::from((
                    [0, 0, 0, 0],
                    on_air_core::DEFAULT_PORT,
                ));
                if let Err(err) = on_air_core::serve_on(addr).await {
                    eprintln!("on-air-core HTTP server failed: {err}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
