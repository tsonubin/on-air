use axum::http::{header, HeaderValue, Method};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

pub mod api;
pub mod cd;
pub mod dsp;
pub mod events;
pub mod http;
pub mod keep_awake;
pub mod mdns;
pub mod net;
pub mod pairing;
pub mod patchbay;
pub mod pipeline;
pub mod sender;
pub mod session;
pub mod settings;
pub mod state;

use state::CoreState;

/// The wire types this core serves (and the TUI reads).
pub use on_air_api::{StatusResponse, DEFAULT_PORT};

/// `sleep_inhibited` is false while the service is off, on platforms
/// without an inhibitor, and when the OS refused it.
pub fn status_for(state: &CoreState) -> StatusResponse {
    StatusResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        service_enabled: state.service_enabled.load(Ordering::Acquire),
        sleep_inhibited: Some(state.sleep_inhibited()),
        lan_addresses: Some(net::lan_addresses()),
    }
}

pub fn build_router(state: CoreState) -> Router {
    let allowed_origins = [
        HeaderValue::from_static("http://127.0.0.1:1420"),
        HeaderValue::from_static("http://localhost:1420"),
        HeaderValue::from_static("tauri://localhost"),
        HeaderValue::from_static("http://tauri.localhost"),
        HeaderValue::from_static("https://tauri.localhost"),
    ];
    let mut router = Router::new()
        .route("/api/status", get(status_handler))
        .route(
            "/api/patchbay",
            get(api::patchbay::snapshot).post(api::patchbay::command),
        )
        .route("/stream/{nonce}/audio.wav", get(api::stream::stream_audio))
        .route("/api/cd", get(api::cd::get_cd))
        .route("/api/cd/control", post(api::cd::control_cd))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route(
            "/api/inputs/active",
            post(api::inputs::activate_input).get(api::inputs::get_active_input),
        )
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route(
            "/api/outputs/active",
            post(api::outputs::activate_output)
                .get(api::outputs::get_active_output)
                .delete(api::outputs::deactivate_output),
        )
        .route(
            "/api/outputs/active/volume",
            post(api::outputs::set_output_volume).get(api::outputs::get_output_volume),
        )
        .route("/api/eq", get(api::eq::get_eq).put(api::eq::set_eq))
        .route(
            "/api/sample-rate",
            get(api::sample_rate::get_sample_rate).put(api::sample_rate::set_sample_rate),
        )
        .route("/api/service", put(api::service::set_service))
        .route("/api/ws", get(api::ws::ws_handler))
        .route("/api/pairing/pin", get(api::pairing::get_pin))
        .route("/api/pairing/verify", post(api::pairing::verify_pin))
        .route("/api/airplay/mode", get(api::airplay::mode))
        .route("/api/airplay/pair", post(api::airplay::pair))
        .route("/api/bluetooth/devices", get(api::bluetooth::list_devices))
        .route("/api/bluetooth/pair", post(api::bluetooth::pair_device))
        .route(
            "/api/bluetooth/connect",
            post(api::bluetooth::connect_device),
        )
        .route(
            "/api/bluetooth/settings",
            post(api::bluetooth::open_settings),
        );
    if state.mock {
        // The disc simulator never exists on a production core.
        router = router.route("/api/mock/cd", post(api::cd::simulate_cd));
    }
    router
        .fallback(api::error::route_not_found)
        .method_not_allowed_fallback(api::error::method_not_allowed)
        .layer(
            CorsLayer::new()
                .allow_origin(allowed_origins)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        )
        .with_state(state)
}

async fn status_handler(
    axum::extract::State(state): axum::extract::State<CoreState>,
) -> Json<StatusResponse> {
    Json(status_for(&state))
}

pub fn mock_mode_enabled() -> bool {
    std::env::var("ON_AIR_MOCK").ok().as_deref() == Some("1")
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let state = if mock_mode_enabled() {
        CoreState::new_mock().await
    } else {
        persistent_state_from_env()
    };
    serve_with_state(listener, state).await
}

/// Where the headless core keeps `settings.json` (and `paired-remotes.json`
/// beside it): `config_dir` when it is absolute (`ON_AIR_CONFIG_DIR`), else
/// `<xdg_config_home>/on-air-core`, else `<home>/.config/on-air-core`.
/// Relative or empty locations are ignored, as the XDG spec asks.
pub fn settings_path_from(
    config_dir: Option<std::path::PathBuf>,
    xdg_config_home: Option<std::path::PathBuf>,
    home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let absolute = |path: Option<std::path::PathBuf>| path.filter(|p| p.is_absolute());
    let dir = absolute(config_dir)
        .or_else(|| absolute(xdg_config_home).map(|dir| dir.join("on-air-core")))
        .or_else(|| absolute(home).map(|home| home.join(".config/on-air-core")))?;
    Some(dir.join("settings.json"))
}

/// The headless core as a user service: the last source, speaker, volume,
/// EQ, rates, service flag and paired phones survive a restart, as they do
/// in the desktop app. With nowhere to keep them it runs from defaults.
fn persistent_state_from_env() -> CoreState {
    let path = settings_path_from(
        std::env::var_os("ON_AIR_CONFIG_DIR").map(Into::into),
        std::env::var_os("XDG_CONFIG_HOME").map(Into::into),
        std::env::var_os("HOME").map(Into::into),
    );
    match path {
        Some(path) => CoreState::new_persistent(path),
        None => {
            eprintln!("on-air-core: no HOME or ON_AIR_CONFIG_DIR; settings will not persist");
            CoreState::new()
        }
    }
}

pub async fn serve_with_state(listener: TcpListener, state: CoreState) -> std::io::Result<()> {
    serve_with_state_until(listener, state, std::future::pending()).await
}

/// Serve until `shutdown` resolves, then stop the audio work, the
/// background tasks and the sleep inhibitor before returning. This is the
/// path `systemctl --user stop` takes; without it the laptop speakers could
/// stay muted. Open connections (a TUI's event socket) are not waited for.
pub async fn serve_with_state_until(
    listener: TcpListener,
    state: CoreState,
    shutdown: impl std::future::Future<Output = ()>,
) -> std::io::Result<()> {
    let port = listener.local_addr()?.port();
    state.serve_port.store(port, Ordering::Release);
    if !state.mock {
        let mut background = state.background.lock();
        background.set_mdns(crate::mdns::spawn_advertisement(
            port,
            env!("CARGO_PKG_VERSION"),
        ));
        for discovery in state.spawn_discovery() {
            background.push(discovery);
        }
        background.push(crate::cd::autoplay::spawn_watch(state.clone()));
    }
    state.spawn_saved_session_restore();
    let patchbay_state = state.clone();
    state.background.lock().push(tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if patchbay_state.patchbay.activated.load(Ordering::Acquire) {
                patchbay_state.patchbay.refresh(&patchbay_state).await;
            }
        }
    }));
    let server = axum::serve(
        listener,
        build_router(state.clone()).into_make_service_with_connect_info::<SocketAddr>(),
    );
    tokio::select! {
        result = std::future::IntoFuture::into_future(server) => result?,
        () = shutdown => {}
    }
    state.shutdown().await;
    state.abort_background_tasks();
    state.keep_awake.set_enabled(false);
    Ok(())
}

/// SIGTERM (systemd stop, logout) or Ctrl-C.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = terminate.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

pub async fn serve_with_state_std(
    listener: std::net::TcpListener,
    state: CoreState,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    serve_with_state(TcpListener::from_std(listener)?, state).await
}

pub async fn serve_on(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}

/// Entry point shared by the `on-air-core` binary and the `serve` example:
/// `PORT` (default [`DEFAULT_PORT`]), `BIND` (default `0.0.0.0` so a Sonos
/// on the LAN can pull the radio stream; set `127.0.0.1` to keep the control
/// plane loopback-only) and `ON_AIR_MOCK=1` for the hardware-free core.
pub fn run_serve() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let host: std::net::IpAddr = std::env::var("BIND")
        .ok()
        .and_then(|b| b.parse().ok())
        .unwrap_or_else(|| std::net::IpAddr::from([0, 0, 0, 0]));
    let addr = SocketAddr::from((host, port));
    eprintln!(
        "on-air-core listening on http://{addr} mock={}",
        mock_mode_enabled()
    );
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime
        .block_on(async {
            let listener = TcpListener::bind(addr).await?;
            let state = if mock_mode_enabled() {
                CoreState::new_mock().await
            } else {
                persistent_state_from_env()
            };
            serve_with_state_until(listener, state, shutdown_signal()).await
        })
        .expect("serve");
}
