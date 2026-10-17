//! Observe actual UFW drops, rather than guessing from its enabled flag.
//! A local client may request a narrowly scoped, administrator-authorized repair.

use crate::state::CoreState;
use on_air_api::FirewallWarning;
use std::net::Ipv4Addr;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|word| word.strip_prefix(key))
}

/// Only accept TCP drops on our actual listening port and a local interface.
/// Other services' drops and public Internet probes are not audio warnings.
pub fn warning_from_log(
    log: &str,
    port: u16,
    interfaces: &[(Ipv4Addr, Ipv4Addr)],
) -> Option<FirewallWarning> {
    for line in log.lines().rev() {
        if !line.contains("[UFW BLOCK]")
            || field(line, "PROTO=") != Some("TCP")
            || field(line, "DPT=").and_then(|s| s.parse::<u16>().ok()) != Some(port)
        {
            continue;
        }
        let Some(source) = field(line, "SRC=").and_then(|s| s.parse::<Ipv4Addr>().ok()) else {
            continue;
        };
        if !source.is_private() {
            continue;
        }
        let destination = field(line, "DST=").and_then(|s| s.parse::<Ipv4Addr>().ok());
        let Some((ip, mask)) = interfaces.iter().find(|(ip, _)| Some(*ip) == destination) else {
            continue;
        };
        let mask = u32::from(*mask);
        let prefix = mask.leading_ones();
        let network = Ipv4Addr::from(u32::from(source) & mask);
        // An unusually broad interface mask must never broaden the repair
        // beyond private addresses. Fall back to just the observed device.
        let subnet = if mask == u32::MAX.checked_shl(32 - prefix).unwrap_or(0)
            && (u32::from(source) & mask) == (u32::from(*ip) & mask)
            && valid_subnet(&format!("{network}/{prefix}"))
        {
            format!("{network}/{prefix}")
        } else {
            format!("{source}/32")
        };
        return Some(FirewallWarning {
            repair_command: format!(
                "sudo ufw allow from {subnet} to any port {port} proto tcp\n\
                 sudo iptables -C ufw-user-input -s {subnet} -p tcp --dport {port} -j ACCEPT || \
                 sudo iptables -I ufw-user-input 1 -s {subnet} -p tcp --dport {port} -j ACCEPT"
            ),
            subnet,
            port,
            message: "UFW is blocking on-air audio from the local network.".into(),
        });
    }
    None
}

pub(crate) fn valid_subnet(subnet: &str) -> bool {
    let Some((ip, prefix)) = subnet.split_once('/') else {
        return false;
    };
    let (Ok(ip), Ok(prefix)) = (ip.parse::<Ipv4Addr>(), prefix.parse::<u32>()) else {
        return false;
    };
    let minimum = match ip.octets() {
        [10, ..] => 8,
        [172, 16..=31, ..] => 12,
        [192, 168, ..] => 16,
        _ => return false,
    };
    (minimum..=32).contains(&prefix)
        && u32::from(ip) & u32::MAX.checked_shl(32 - prefix).unwrap_or(0) == u32::from(ip)
}

pub fn spawn_watch(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let repaired_at = state.firewall_repaired_at.load(Ordering::Acquire);
            let warning = if state.service_enabled.load(Ordering::Acquire)
                && state.stream_clients.load(Ordering::Acquire) == 0
            {
                inspect(state.serve_port.load(Ordering::Acquire), repaired_at).await
            } else {
                None
            };
            {
                let mut current = state.firewall_warning.lock();
                if repaired_at == state.firewall_repaired_at.load(Ordering::Acquire) {
                    *current = warning;
                }
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    })
}

#[cfg(target_os = "linux")]
async fn inspect(port: u16, repaired_at: u64) -> Option<FirewallWarning> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let since = format!("@{}", now.saturating_sub(90).max(repaired_at));
    let output = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new("journalctl")
            .args([
                "--kernel",
                "--no-pager",
                "--since",
                &since,
                "--grep",
                "UFW BLOCK",
                "--lines",
                "32",
                "--output",
                "cat",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let interfaces: Vec<_> = if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(addr) => Some((addr.ip, addr.netmask)),
            _ => None,
        })
        .collect();
    warning_from_log(&String::from_utf8_lossy(&output.stdout), port, &interfaces)
}

#[cfg(not(target_os = "linux"))]
async fn inspect(_port: u16, _repaired_at: u64) -> Option<FirewallWarning> {
    None
}
