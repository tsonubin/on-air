use futures_util::StreamExt;
use on_air_core::api::ws::WsEvent;
use on_air_core::state::CoreState;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn ws_forwards_published_events_as_json() {
    let state = CoreState::new();
    let ws_tx = state.ws_tx.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (mut ws_stream, _) = connect_async(format!("ws://{addr}/api/ws")).await.unwrap();

    let event = WsEvent::LevelMeter { rms: 0.5, peak: 0.9 };
    let _ = ws_tx.send(event.clone());

    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next())
        .await
        .expect("received a message before timing out")
        .expect("stream not closed")
        .expect("message read ok");

    let Message::Text(text) = msg else {
        panic!("expected a text message, got {msg:?}");
    };
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["type"], "LevelMeter");
    assert_eq!(parsed["rms"], 0.5);

    let _ = ws_stream.close(None).await;
}
