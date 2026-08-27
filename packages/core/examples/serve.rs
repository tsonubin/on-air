use std::net::{IpAddr, SocketAddr};

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(on_air_core::DEFAULT_PORT);
    // Default 0.0.0.0 so a Sonos on the LAN can GET /stream/audio.wav.
    // Set BIND=127.0.0.1 to keep the control plane loopback-only.
    let host: IpAddr = std::env::var("BIND")
        .ok()
        .and_then(|b| b.parse().ok())
        .unwrap_or_else(|| IpAddr::from([0, 0, 0, 0]));
    let addr = SocketAddr::from((host, port));
    eprintln!(
        "on-air-core listening on http://{addr} mock={}",
        on_air_core::mock_mode_enabled()
    );
    on_air_core::serve_on(addr).await.expect("serve");
}
