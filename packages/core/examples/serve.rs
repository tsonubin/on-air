use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(on_air_core::DEFAULT_PORT);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    eprintln!("on-air-core listening on http://{addr} mock={}", on_air_core::mock_mode_enabled());
    on_air_core::serve_on(addr).await.expect("serve");
}
