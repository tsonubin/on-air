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
    let app = on_air_core::build_router(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (mut ws_stream, _) = connect_async(format!("ws://{addr}/api/ws")).await.unwrap();

    let event = WsEvent::LevelMeter {
        left: on_air_api::MeterLevel {
            rms: 0.5,
            peak: 0.9,
        },
        right: on_air_api::MeterLevel {
            rms: 0.25,
            peak: 0.4,
        },
    };
    // keep publishing until a subscriber (the websocket handler) exists
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if ws_tx.send(event.clone()).is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("subscriber appeared before timeout");

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
    assert_eq!(
        parsed,
        serde_json::json!({
            "type": "LevelMeter",
            "left": {"rms": 0.5, "peak": 0.9},
            "right": {"rms": 0.25, "peak": 0.4},
        })
    );

    let _ = ws_stream.close(None).await;
}

/// Browsers and React Native never surface protocol pings, so an idle core
/// must send a text frame that a client's stall timer can see.
#[tokio::test]
async fn idle_ws_sends_a_text_heartbeat() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(CoreState::new())
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (mut ws_stream, _) = connect_async(format!("ws://{addr}/api/ws")).await.unwrap();
    let deadline = on_air_core::api::ws::HEARTBEAT_INTERVAL + std::time::Duration::from_secs(2);
    let text = tokio::time::timeout(deadline, async {
        loop {
            match ws_stream.next().await {
                Some(Ok(Message::Text(text))) => break text,
                Some(Ok(_)) => continue,
                other => panic!("socket ended before a heartbeat: {other:?}"),
            }
        }
    })
    .await
    .expect("a text heartbeat within the interval");
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, serde_json::json!({ "type": "Heartbeat" }));

    let _ = ws_stream.close(None).await;
}

#[test]
fn heartbeat_event_is_a_bare_tagged_json() {
    let json = serde_json::to_value(WsEvent::Heartbeat).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "Heartbeat" }));
}

#[test]
fn service_state_event_is_tagged_json() {
    let event = WsEvent::ServiceStateChanged { enabled: false };
    let json = serde_json::to_value(event).unwrap();
    assert_eq!(json["type"], "ServiceStateChanged");
    assert_eq!(json["enabled"], false);
}

#[test]
fn cd_state_event_is_tagged_json() {
    let event = WsEvent::CdStateChanged {
        present: true,
        playing: true,
        track: 3,
        track_count: 12,
        title: Some("So What".into()),
        album: Some("Kind of Blue".into()),
        position_ms: 12_000,
        duration_ms: 180_000,
    };
    let json = serde_json::to_value(event).unwrap();
    assert_eq!(json["type"], "CdStateChanged");
    assert_eq!(json["track"], 3);
    assert_eq!(json["title"], "So What");
}

#[tokio::test]
async fn a_paused_service_still_streams_events_so_clients_see_it_come_back() {
    let state = CoreState::new_mock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state.clone())
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    state.begin_service_transition(false);

    let (mut ws_stream, _) = connect_async(format!("ws://{addr}/api/ws"))
        .await
        .expect("the event socket opens while the service is paused");

    let ws_tx = state.ws_tx.clone();
    let turned_on = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if ws_tx.receiver_count() > 0 {
                state.begin_service_transition(true);
            }
            if let Ok(Some(Ok(Message::Text(text)))) =
                tokio::time::timeout(std::time::Duration::from_millis(50), ws_stream.next()).await
            {
                if text.contains("ServiceStateChanged") {
                    break text.to_string();
                }
            }
        }
    })
    .await
    .expect("ServiceStateChanged reaches a client connected while paused");
    assert!(turned_on.contains("\"enabled\":true"), "{turned_on}");
}
