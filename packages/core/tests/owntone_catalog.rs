use axum::Json;
use axum::routing::get;
use axum::Router;
use on_air_core::sender::airplay::fetch_owntone_outputs;
use tokio::net::TcpListener;

#[tokio::test]
async fn fetch_owntone_outputs_maps_sidecar_json_into_catalog() {
    let app = Router::new().route(
        "/api/outputs",
        get(|| async {
            Json(serde_json::json!({
                "outputs": [
                    {"id": "1", "name": "HomePod"},
                    {"id": "2", "name": "Apple TV"}
                ]
            }))
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;

    let devices = fetch_owntone_outputs(&format!("http://{addr}")).await.unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].name, "HomePod");
    assert_eq!(devices[1].id, "2");
}
