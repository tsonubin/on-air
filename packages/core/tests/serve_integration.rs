use tokio::net::TcpListener;

#[tokio::test]
async fn serve_responds_to_status_over_http() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        on_air_core::serve(listener).await.unwrap();
    });

    let url = format!("http://{addr}/api/status");
    let response = reqwest::get(url).await.unwrap();
    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}
