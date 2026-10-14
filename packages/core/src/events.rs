//! Events the core broadcasts to WebSocket clients. Domain modules publish
//! them; `api::ws` only forwards them, so nothing below the API layer needs
//! to import `crate::api`.

use std::collections::HashSet;
use tokio::sync::broadcast;

pub use on_air_api::WsEvent;

/// `DeviceJoined`/`DeviceLeft` for whatever changed between two catalogs of
/// `(id, name)` pairs.
pub(crate) fn emit_catalog_diff(
    ws_tx: &broadcast::Sender<WsEvent>,
    transport: &str,
    before: &[(String, String)],
    after: &[(String, String)],
) {
    let before_ids: HashSet<&str> = before.iter().map(|(id, _)| id.as_str()).collect();
    let after_ids: HashSet<&str> = after.iter().map(|(id, _)| id.as_str()).collect();
    for (id, name) in after {
        if !before_ids.contains(id.as_str()) {
            let _ = ws_tx.send(WsEvent::DeviceJoined {
                transport: transport.to_string(),
                id: id.clone(),
                name: name.clone(),
            });
        }
    }
    for (id, _) in before {
        if !after_ids.contains(id.as_str()) {
            let _ = ws_tx.send(WsEvent::DeviceLeft {
                transport: transport.to_string(),
                id: id.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_diff_emits_joined_and_left_for_any_transport() {
        let (ws_tx, mut rx) = broadcast::channel(8);
        let before = vec![("a".to_string(), "A".to_string())];
        let after = vec![("b".to_string(), "B".to_string())];
        emit_catalog_diff(&ws_tx, "airplay", &before, &after);
        match rx.try_recv().unwrap() {
            WsEvent::DeviceJoined {
                transport,
                id,
                name,
            } => {
                assert_eq!(
                    (transport.as_str(), id.as_str(), name.as_str()),
                    ("airplay", "b", "B")
                );
            }
            other => panic!("expected DeviceJoined, got {other:?}"),
        }
        match rx.try_recv().unwrap() {
            WsEvent::DeviceLeft { transport, id } => {
                assert_eq!((transport.as_str(), id.as_str()), ("airplay", "a"));
            }
            other => panic!("expected DeviceLeft, got {other:?}"),
        }
    }

    #[test]
    fn input_state_changed_omits_a_missing_error() {
        let json = serde_json::to_value(WsEvent::InputStateChanged {
            name: Some("Mic".into()),
            active: true,
            error: None,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "InputStateChanged", "name": "Mic", "active": true})
        );
    }
}
