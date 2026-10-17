use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use on_air_core::{firewall::warning_from_log, state::CoreState};
use std::net::{Ipv4Addr, SocketAddr};
use tower::ServiceExt;

const BLOCK: &str =
    "[UFW BLOCK] IN=wlp3s0 SRC=192.168.5.7 DST=192.168.5.101 PROTO=TCP SPT=47000 DPT=47990 SYN";

fn warning() -> on_air_api::FirewallWarning {
    warning_from_log(
        BLOCK,
        47990,
        &[(
            Ipv4Addr::new(192, 168, 5, 101),
            Ipv4Addr::new(255, 255, 255, 0),
        )],
    )
    .unwrap()
}

#[test]
fn the_reported_sonos_drop_produces_a_port_and_subnet_scoped_repair() {
    let warning = warning();
    assert_eq!(warning.subnet, "192.168.5.0/24");
    assert_eq!(warning.port, 47990);
    assert!(warning
        .repair_command
        .contains("sudo ufw allow from 192.168.5.0/24 to any port 47990 proto tcp"));
    // Also repair the actual failure: UFW disabled in config, but its kernel
    // rules still running, so `ufw allow` alone only saves the rule.
    assert!(warning
        .repair_command
        .contains("iptables -I ufw-user-input"));
}

#[test]
fn unrelated_drops_and_unreadable_logs_do_not_blame_ufw() {
    let interfaces = [(
        Ipv4Addr::new(192, 168, 5, 101),
        Ipv4Addr::new(255, 255, 255, 0),
    )];
    for log in [
        String::new(),
        "Permission denied".into(),
        BLOCK.replace("47990", "22"),
        BLOCK.replace("PROTO=TCP", "PROTO=UDP"),
        BLOCK.replace("[UFW BLOCK]", "[UFW ALLOW]"),
        BLOCK.replace("192.168.5.7", "8.8.8.8"),
        BLOCK.replace("192.168.5.101", "192.168.5.102"),
    ] {
        assert!(
            warning_from_log(&log, 47990, &interfaces).is_none(),
            "{log}"
        );
    }
}

#[test]
fn custom_ports_and_real_interface_masks_are_used() {
    let interfaces = [(
        Ipv4Addr::new(192, 168, 5, 101),
        Ipv4Addr::new(255, 255, 0, 0),
    )];
    let warning = warning_from_log(&BLOCK.replace("47990", "48000"), 48000, &interfaces).unwrap();
    assert_eq!(warning.subnet, "192.168.0.0/16");
    assert_eq!(warning.port, 48000);
    let broad = [(Ipv4Addr::new(192, 168, 5, 101), Ipv4Addr::UNSPECIFIED)];
    assert_eq!(
        warning_from_log(BLOCK, 47990, &broad).unwrap().subnet,
        "192.168.5.7/32"
    );
}

#[tokio::test]
async fn status_shares_the_warning_and_a_local_confirmed_repair_clears_it() {
    let state = CoreState::new_mock().await;
    *state.firewall_warning.lock() = Some(warning());
    assert!(on_air_core::status_for(&state).firewall_warning.is_some());
    let app = on_air_core::build_router(state.clone());
    let response = app
        .oneshot(
            Request::post("/api/network/firewall")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(on_air_core::status_for(&state).firewall_warning.is_none());
}

#[tokio::test]
async fn a_paired_remote_can_read_diagnostics_but_cannot_request_host_privileges() {
    let state = CoreState::new_mock().await;
    let token = state
        .pairing()
        .verify(on_air_core::pairing::MOCK_PIN)
        .unwrap();
    *state.firewall_warning.lock() = Some(warning());
    let app = on_air_core::build_router(state.clone());
    let request = Request::post("/api/network/firewall")
        .header("authorization", format!("Bearer {token}"))
        .extension(ConnectInfo(
            "192.168.5.12:1234".parse::<SocketAddr>().unwrap(),
        ))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(state.firewall_warning.lock().is_some());
}

#[tokio::test]
async fn repair_never_accepts_an_arbitrary_subnet_or_port() {
    let state = CoreState::new_mock().await;
    let mut warning = warning();
    warning.subnet = "0.0.0.0/0".into();
    *state.firewall_warning.lock() = Some(warning.clone());
    let app = on_air_core::build_router(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/network/firewall")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    warning.subnet = "192.168.5.0/24".into();
    warning.port = 22;
    *state.firewall_warning.lock() = Some(warning);
    let response = app
        .oneshot(
            Request::post("/api/network/firewall")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
