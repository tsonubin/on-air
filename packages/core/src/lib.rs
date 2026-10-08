use axum::http::{header, HeaderValue, Method};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

pub mod api;
pub mod cd;
pub mod dsp;
pub mod events;
pub mod http;
pub mod mdns;
pub mod net;
pub mod pairing;
pub mod pipeline;
pub mod sender;
pub mod session;
pub mod settings;
pub mod state;

use state::CoreState;

/// The pairing extractors live with the HTTP layer; re-exported at the old path.
pub use api::auth;

// Keep in sync with DEFAULT_PORT in packages/api-types/src/index.ts
pub const DEFAULT_PORT: u16 = 47990;

#[derive(Serialize, PartialEq, Debug)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub service_enabled: bool,
    /// Non-loopback IPv4 addresses of this host, so the desktop can show the
    /// address a phone must type. Empty when unknown.
    pub lan_addresses: Vec<String>,
}

pub fn status() -> StatusResponse {
    status_with_service(true)
}

pub fn status_with_service(service_enabled: bool) -> StatusResponse {
    StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        service_enabled,
        lan_addresses: net::lan_addresses(),
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
        .route("/stream/:nonce/audio.wav", get(api::stream::stream_audio))
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
    Json(status_with_service(
        state.service_enabled.load(Ordering::Acquire),
    ))
}

pub fn mock_mode_enabled() -> bool {
    std::env::var("ON_AIR_MOCK").ok().as_deref() == Some("1")
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let state = if mock_mode_enabled() {
        CoreState::new_mock().await
    } else {
        CoreState::new()
    };
    serve_with_state(listener, state).await
}

pub async fn serve_with_state(listener: TcpListener, state: CoreState) -> std::io::Result<()> {
    let port = listener.local_addr()?.port();
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
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
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
    runtime.block_on(serve_on(addr)).expect("serve");
}
