//! A Linux routing adapter. Pulse/PipeWire owns local mixing and fan-out;
//! each network sink gets an isolated instance of the existing sender and
//! capture sessions. The legacy exclusive API continues to own its session.
pub(crate) mod pulse;

use crate::{session, state::CoreState};
use on_air_api::{
    OutputInfo, PatchbayCommand, PatchbayLink, PatchbayNode, PatchbaySide, PatchbaySnapshot,
};
use pulse::{bus_name, Catalog, Pulse};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

const DESKTOP: &str = "desktop";
const MAX_GROUPS: usize = 16;
const MAX_LINKS: usize = 64;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    groups: Vec<Group>,
    links: Vec<Link>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Group {
    id: String,
    name: String,
    side: PatchbaySide,
    members: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Link {
    source: String,
    target: String,
}

#[derive(Clone)]
struct Endpoint {
    node: PatchbayNode,
    address: String,
    /// A monitor reads a sink. This identity lets cycle detection see
    /// feedback through a monitor, including through several groups.
    origin: String,
}

struct Inner {
    saved: Saved,
    path: Option<PathBuf>,
    pulse: Pulse,
    endpoints: Vec<Endpoint>,
    networks: HashMap<String, (OutputInfo, CoreState)>,
    running_sinks: Vec<String>,
    snapshot: PatchbaySnapshot,
}

pub struct Patchbay {
    pub activated: AtomicBool,
    transition: Mutex<()>,
    inner: Mutex<Inner>,
}

impl Patchbay {
    pub fn new(path: Option<PathBuf>, mock: bool) -> Self {
        let saved = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .unwrap_or_default();
        let activated = path.as_ref().is_some_and(|p| p.exists());
        Self {
            activated: AtomicBool::new(activated),
            transition: Mutex::new(()),
            inner: Mutex::new(Inner {
                saved,
                path,
                pulse: Pulse::new(mock),
                endpoints: Vec::new(),
                networks: HashMap::new(),
                running_sinks: Vec::new(),
                snapshot: PatchbaySnapshot {
                    supported: cfg!(target_os = "linux") || mock,
                    enabled: true,
                    nodes: Vec::new(),
                    links: Vec::new(),
                    error: None,
                },
            }),
        }
    }

    pub async fn snapshot(&self) -> PatchbaySnapshot {
        self.inner.lock().await.snapshot.clone()
    }

    pub async fn stream_state(&self, nonce: &str) -> Option<CoreState> {
        self.inner
            .lock()
            .await
            .networks
            .values()
            .find_map(|(_, child)| {
                child
                    .output()
                    .snapshot()
                    .serves_radio(nonce)
                    .then(|| child.clone())
            })
    }

    pub async fn uses_receiver(&self, transport: &str, id: &str) -> bool {
        self.inner.lock().await.networks.values().any(|(_, child)| {
            child
                .active_output()
                .is_some_and(|active| active.transport == transport && active.device_id == id)
        })
    }

    pub async fn refresh(&self, state: &CoreState) {
        let _transition = self.transition.lock().await;
        self.refresh_locked(state).await;
    }

    async fn refresh_locked(&self, state: &CoreState) {
        let enabled = state.service_enabled.load(Ordering::Acquire);
        let discovered: Vec<_> = session::list(state)
            .await
            .into_iter()
            .filter(|d| matches!(d.transport.as_str(), "airplay" | "sonos"))
            .take(32)
            .collect();
        let mut inner = self.inner.lock().await;
        inner.snapshot.enabled = enabled;
        if !inner.snapshot.supported {
            inner.snapshot.error =
                Some("The patchbay requires Linux with PipeWire or PulseAudio.".into());
            return;
        }
        if let Err(error) = inner.reconcile(state, &discovered, enabled).await {
            inner.snapshot.error = Some(error);
            return;
        }
        let work: Vec<_> = inner
            .networks
            .iter()
            .map(|(id, (info, child))| {
                let endpoint = inner.endpoints.iter().find(|e| e.node.id == *id);
                let bus = bus_name(id);
                let linked = inner.snapshot.links.iter().any(|link| {
                    link.state == "connected" && inner.leaves(&link.target).contains(id)
                });
                let direct = inner.catalog_running(&bus);
                (
                    id.clone(),
                    info.clone(),
                    child.clone(),
                    enabled
                        && (linked || direct)
                        && endpoint.is_some_and(|e| e.node.state != "unavailable"),
                )
            })
            .collect();
        drop(inner);
        // Do not hold `inner` while starting a sender: Sonos may immediately
        // GET its nonce, whose HTTP handler needs to find this child here.
        for (id, info, child, wanted) in work {
            let mut failure = None;
            let needs_cleanup = child.output().snapshot().phase == session::OutputPhase::Failed
                || child.input().snapshot().error.is_some();
            if wanted && needs_cleanup {
                if let Err(error) = child.output().deactivate().await {
                    failure = Some(error.to_string());
                }
                child.input().stop().await;
            }
            if wanted && failure.is_none() && child.active_output().is_none() {
                let claimed = state.active_output().is_some_and(|active| {
                    active.device_id == info.id && active.transport == info.transport
                }) || state.sync.uses_receiver(&info.transport, &info.id);
                if claimed {
                    failure = Some("This speaker is in use by the terminal or another client. Stop that output first.".into());
                } else if info.needs_pair && !info.paired {
                    failure = Some("Pair this speaker in on-air before connecting it.".into());
                } else {
                    let source = format!("{}.monitor", bus_name(&id));
                    if state.mock {
                        child.input().set_mock_inputs(vec![source.clone()]);
                    }
                    if let Err(error) = child.input().activate(&source).await {
                        failure = Some(error.to_string());
                    } else if let Err(error) =
                        session::activate(&child, &info.transport, &info.id).await
                    {
                        child.input().stop().await;
                        failure = Some(error.to_string());
                    }
                }
            } else if !wanted && child.active_output().is_some() {
                if let Err(error) = child.output().deactivate().await {
                    failure = Some(error.to_string());
                }
                child.input().stop().await;
            }
            let mut inner = self.inner.lock().await;
            let output = child.output().snapshot();
            if failure.is_none() {
                failure = output.error.clone();
            }
            let phase = if failure.is_some() || output.phase == session::OutputPhase::Failed {
                "failed"
            } else if wanted && output.phase == session::OutputPhase::Live {
                "playing"
            } else if wanted && output.phase == session::OutputPhase::Starting {
                "starting"
            } else {
                "available"
            };
            if let Some(endpoint) = inner.endpoints.iter_mut().find(|e| e.node.id == id) {
                if endpoint.node.state != "unavailable" {
                    endpoint.node.state = phase.into();
                }
                if let Some(error) = failure {
                    endpoint.node.detail = error;
                }
            }
        }
        let mut inner = self.inner.lock().await;
        inner.snapshot.nodes = inner.endpoints.iter().map(|e| e.node.clone()).collect();
        // Report transport failures on affected cables, including groups.
        let links = inner.snapshot.links.clone();
        inner.snapshot.links = links
            .into_iter()
            .map(|mut link| {
                for id in inner.leaves(&link.target) {
                    if let Some(node) = inner
                        .endpoints
                        .iter()
                        .find(|e| e.node.id == id)
                        .map(|e| &e.node)
                    {
                        if matches!(node.state.as_str(), "failed" | "unavailable") {
                            link.state = node.state.clone();
                            link.error = Some(node.detail.clone());
                        }
                    }
                }
                link
            })
            .collect();
    }

    pub async fn command(
        &self,
        state: &CoreState,
        command: PatchbayCommand,
    ) -> Result<PatchbaySnapshot, String> {
        self.activated.store(true, Ordering::Release);
        let _transition = self.transition.lock().await;
        self.refresh_locked(state).await;
        {
            let mut inner = self.inner.lock().await;
            if !state.service_enabled.load(Ordering::Acquire) {
                return Err("The on-air service is paused.".into());
            }
            if let Some(error) = inner.snapshot.error.clone() {
                return Err(error);
            }
            let previous = inner.saved.clone();
            if let Err(error) = inner.apply(command).await {
                inner.saved = previous;
                return Err(error);
            }
            if let Err(error) = inner.persist() {
                inner.saved = previous;
                return Err(error);
            }
        }
        self.refresh_locked(state).await;
        Ok(self.snapshot().await)
    }

    pub async fn stop(&self) {
        let _transition = self.transition.lock().await;
        let mut inner = self.inner.lock().await;
        if let Err(error) = inner.pulse.retain_links(&[]).await {
            eprintln!("could not suspend patchbay links: {error}");
        }
        let children: Vec<_> = inner.networks.values().map(|(_, c)| c.clone()).collect();
        drop(inner);
        for child in children {
            if let Err(error) = child.output().deactivate().await {
                eprintln!("could not stop patchbay speaker: {error}");
                child.output().abandon().await;
            }
            child.input().stop().await;
        }
    }
}

impl Inner {
    fn endpoint(&self, id: &str) -> Result<&Endpoint, String> {
        self.endpoints
            .iter()
            .find(|e| e.node.id == id)
            .ok_or_else(|| "That device is no longer available.".into())
    }

    fn leaves(&self, id: &str) -> Vec<String> {
        self.saved
            .groups
            .iter()
            .find(|g| g.id == id)
            .map(|g| g.members.clone())
            .unwrap_or_else(|| vec![id.into()])
    }

    fn catalog_running(&self, bus: &str) -> bool {
        self.running_sinks.iter().any(|name| name == bus)
    }

    async fn reconcile(
        &mut self,
        state: &CoreState,
        discovered: &[OutputInfo],
        enabled: bool,
    ) -> Result<(), String> {
        self.snapshot.error = None;
        let catalog = self.pulse.catalog().await?;
        self.pulse.bus(DESKTOP, "On-Air desktop").await?;
        let stale: Vec<_> = self
            .networks
            .iter()
            .filter(|(id, (info, child))| {
                !discovered
                    .iter()
                    .any(|d| d.id == info.id && d.transport == info.transport)
                    && child.active_output().is_none()
                    && catalog.default_output != bus_name(id)
                    && !catalog.running_sinks.contains(&bus_name(id))
                    && !self
                        .saved
                        .links
                        .iter()
                        .any(|link| self.leaves(&link.target).contains(id))
                    && !self
                        .saved
                        .groups
                        .iter()
                        .any(|group| group.members.contains(id))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            self.pulse.unload(&bus_name(&id)).await?;
            self.networks.remove(&id);
        }
        for device in discovered {
            let id = format!("network:{}:{}", device.transport, device.id);
            if !self.networks.contains_key(&id) && self.networks.len() >= 32 {
                continue;
            }
            self.pulse.bus(&id, &device.name).await?;
            self.networks
                .entry(id)
                .and_modify(|(info, _)| *info = device.clone())
                .or_insert_with(|| (device.clone(), state.patchbay_child()));
        }
        for group in &self.saved.groups {
            self.pulse.bus(&group.id, &group.name).await?;
        }
        self.endpoints = self.local_endpoints(&catalog);
        let desktop_bus = bus_name(DESKTOP);
        self.add_bus_endpoint(
            DESKTOP.into(),
            "Desktop".into(),
            PatchbaySide::Input,
            "monitor",
            &desktop_bus,
            &catalog,
            vec![],
            "Select On-Air desktop as system output to capture application audio.",
        );
        self.add_bus_endpoint(
            "desktop-output".into(),
            "On-Air desktop".into(),
            PatchbaySide::Output,
            "local",
            &desktop_bus,
            &catalog,
            vec![],
            "System audio enters the Desktop source here.",
        );
        // HashMap order would reshuffle the panel on every poll.
        let mut networks: Vec<_> = self.networks.clone().into_iter().collect();
        networks.sort_by(|(_, (a, _)), (_, (b, _))| a.name.cmp(&b.name));
        for (id, (info, _)) in &networks {
            let present = discovered
                .iter()
                .any(|d| d.transport == info.transport && d.id == info.id);
            self.add_bus_endpoint(
                id.clone(),
                info.name.clone(),
                PatchbaySide::Output,
                &info.transport,
                &bus_name(id),
                &catalog,
                vec![],
                "Network speaker · independent timing; live microphone monitoring will be delayed.",
            );
            if !present {
                let node = &mut self.endpoints.last_mut().unwrap().node;
                node.state = "unavailable".into();
                node.detail =
                    "Speaker unavailable; this route will reconnect when it returns.".into();
            }
        }
        for group in self.saved.groups.clone() {
            self.add_bus_endpoint(
                group.id.clone(),
                group.name,
                group.side,
                "group",
                &bus_name(&group.id),
                &catalog,
                group.members,
                if group.side == PatchbaySide::Output {
                    "Output group · independent timing"
                } else {
                    "Input mix"
                },
            );
        }
        self.validate_graph()?;
        let mut wanted = Vec::new();
        let mut failures = HashMap::new();
        if enabled {
            for group in self.saved.groups.clone() {
                let bus = bus_name(&group.id);
                let gain = if group.side == PatchbaySide::Input {
                    100 / group.members.len().max(1) as u8
                } else {
                    100
                };
                for member in group.members {
                    let edge = self.endpoint(&member).map(|e| {
                        if group.side == PatchbaySide::Input {
                            (e.address.clone(), bus.clone())
                        } else {
                            (format!("{bus}.monitor"), e.address.clone())
                        }
                    });
                    match edge {
                        Ok((source, target)) => match self.pulse.link(&source, &target, gain).await
                        {
                            Ok(key) => wanted.push(key),
                            Err(error) => {
                                failures.insert(group.id.clone(), error);
                            }
                        },
                        Err(error) => {
                            failures.insert(group.id.clone(), error);
                        }
                    }
                }
            }
        }
        self.snapshot.links.clear();
        for link in self.saved.links.clone() {
            let endpoints = self.endpoint(&link.source).and_then(|a| {
                self.endpoint(&link.target)
                    .map(|b| (a.address.clone(), b.address.clone()))
            });
            let mut error = failures
                .get(&link.source)
                .or_else(|| failures.get(&link.target))
                .cloned();
            if enabled && error.is_none() {
                match endpoints {
                    Ok((source, target)) => match self.pulse.link(&source, &target, 100).await {
                        Ok(key) => wanted.push(key),
                        Err(reason) => error = Some(reason),
                    },
                    Err(reason) => error = Some(reason),
                }
            }
            self.snapshot.links.push(PatchbayLink {
                source: link.source,
                target: link.target,
                state: if !enabled {
                    "paused"
                } else if error.is_some() {
                    "unavailable"
                } else {
                    "connected"
                }
                .into(),
                error,
            });
        }
        self.pulse.retain_links(&wanted).await?;
        self.running_sinks = catalog.running_sinks;
        self.snapshot.nodes = self.endpoints.iter().map(|e| e.node.clone()).collect();
        // Remember opt-in even when the user only selects virtual speakers
        // in system settings, without creating an explicit patch link.
        if self.path.as_ref().is_some_and(|path| !path.exists()) {
            self.persist()?;
        }
        Ok(())
    }

    fn local_endpoints(&self, catalog: &Catalog) -> Vec<Endpoint> {
        catalog
            .devices
            .iter()
            .filter(|d| !d.name.starts_with("on_air_"))
            .map(|d| {
                let is_input = d.side == PatchbaySide::Input;
                Endpoint {
                    node: PatchbayNode {
                        id: format!("{}:{}", if is_input { "source" } else { "sink" }, d.name),
                        name: d.label.clone(),
                        side: d.side,
                        kind: if d.monitor_of.is_some() {
                            "monitor"
                        } else {
                            "local"
                        }
                        .into(),
                        members: vec![],
                        volume: d.volume,
                        muted: d.muted,
                        is_default: &d.name
                            == if is_input {
                                &catalog.default_input
                            } else {
                                &catalog.default_output
                            },
                        state: "available".into(),
                        detail: d.name.clone(),
                    },
                    address: d.name.clone(),
                    origin: d
                        .monitor_of
                        .clone()
                        .unwrap_or_else(|| format!("source:{}", d.name)),
                }
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn add_bus_endpoint(
        &mut self,
        id: String,
        name: String,
        side: PatchbaySide,
        kind: &str,
        bus: &str,
        catalog: &Catalog,
        members: Vec<String>,
        detail: &str,
    ) {
        let address = if side == PatchbaySide::Input {
            format!("{bus}.monitor")
        } else {
            bus.into()
        };
        let device = catalog.devices.iter().find(|d| d.name == address);
        self.endpoints.push(Endpoint {
            node: PatchbayNode {
                id,
                name,
                side,
                kind: kind.into(),
                members,
                volume: device.map_or(100, |d| d.volume),
                muted: device.is_some_and(|d| d.muted),
                is_default: &address
                    == if side == PatchbaySide::Input {
                        &catalog.default_input
                    } else {
                        &catalog.default_output
                    },
                state: "available".into(),
                detail: detail.into(),
            },
            address,
            origin: bus.into(),
        });
    }

    fn validate_graph(&self) -> Result<(), String> {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for group in &self.saved.groups {
            for member in &group.members {
                if let Ok(e) = self.endpoint(member) {
                    let (a, b) = if group.side == PatchbaySide::Input {
                        (e.origin.clone(), bus_name(&group.id))
                    } else {
                        (bus_name(&group.id), e.address.clone())
                    };
                    edges.entry(a).or_default().push(b);
                }
            }
        }
        for link in &self.saved.links {
            if let (Ok(a), Ok(b)) = (self.endpoint(&link.source), self.endpoint(&link.target)) {
                edges
                    .entry(a.origin.clone())
                    .or_default()
                    .push(b.address.clone());
            }
        }
        if has_cycle(&edges) {
            Err("This connection would create an audio feedback loop.".into())
        } else {
            Ok(())
        }
    }

    async fn apply(&mut self, command: PatchbayCommand) -> Result<(), String> {
        match command {
            PatchbayCommand::Connect { source, target } => {
                if self.endpoint(&source)?.node.side != PatchbaySide::Input
                    || self.endpoint(&target)?.node.side != PatchbaySide::Output
                {
                    return Err("Connect an input to an output.".into());
                }
                let link = Link { source, target };
                if !self.saved.links.contains(&link) {
                    if self.saved.links.len() >= MAX_LINKS {
                        return Err("The patchbay supports up to 64 connections.".into());
                    }
                    self.saved.links.push(link);
                }
                self.validate_graph()?;
            }
            PatchbayCommand::Disconnect { source, target } => self
                .saved
                .links
                .retain(|l| l.source != source || l.target != target),
            PatchbayCommand::Combine { side, members } => {
                if !(2..=16).contains(&members.len()) {
                    return Err("Combine between 2 and 16 items.".into());
                }
                let mut leaves = Vec::new();
                for member in &members {
                    if self.endpoint(member)?.node.side != side {
                        return Err("Combine items on the same side.".into());
                    }
                    leaves.extend(self.leaves(member));
                }
                leaves.sort();
                leaves.dedup();
                if !(2..=16).contains(&leaves.len()) {
                    return Err("A group needs 2 to 16 distinct members.".into());
                }
                if let Some(existing) = members
                    .iter()
                    .rev()
                    .find(|id| self.saved.groups.iter().any(|g| &g.id == *id))
                    .cloned()
                {
                    self.saved
                        .groups
                        .iter_mut()
                        .find(|g| g.id == existing)
                        .unwrap()
                        .members = leaves;
                } else {
                    if self.saved.groups.len() >= MAX_GROUPS {
                        return Err("The patchbay supports up to 16 groups.".into());
                    }
                    let id = format!("group:{}", session::new_stream_nonce());
                    let stem = if side == PatchbaySide::Input {
                        "Mix"
                    } else {
                        "Rooms"
                    };
                    self.saved.groups.push(Group {
                        id,
                        name: format!("{stem} {}", self.saved.groups.len() + 1),
                        side,
                        members: leaves,
                    });
                }
                self.validate_graph()?;
            }
            PatchbayCommand::RemoveGroup { id } => {
                if !self.saved.groups.iter().any(|group| group.id == id) {
                    return Err("That group no longer exists.".into());
                }
                self.saved.groups.retain(|g| g.id != id);
                self.saved
                    .links
                    .retain(|l| l.source != id && l.target != id);
                self.pulse.unload(&bus_name(&id)).await?;
            }
            PatchbayCommand::SetDefault { id } => {
                let endpoint = self.endpoint(&id)?.clone();
                self.pulse
                    .control(endpoint.node.side, &endpoint.address, "default", None)
                    .await?;
            }
            PatchbayCommand::SetVolume { id, volume } => {
                if volume > 100 {
                    return Err("Volume must be between 0 and 100.".into());
                }
                let endpoint = self.endpoint(&id)?.clone();
                self.pulse
                    .control(
                        endpoint.node.side,
                        &endpoint.address,
                        "volume",
                        Some(format!("{volume}%")),
                    )
                    .await?;
            }
            PatchbayCommand::SetMuted { id, muted } => {
                let endpoint = self.endpoint(&id)?.clone();
                self.pulse
                    .control(
                        endpoint.node.side,
                        &endpoint.address,
                        "mute",
                        Some(if muted { "1" } else { "0" }.into()),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let parent = path.parent().ok_or("Invalid patchbay settings path")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&self.saved).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    }
}

fn has_cycle(edges: &HashMap<String, Vec<String>>) -> bool {
    fn visit<'a>(
        node: &'a str,
        edges: &'a HashMap<String, Vec<String>>,
        visiting: &mut HashSet<&'a str>,
        done: &mut HashSet<&'a str>,
    ) -> bool {
        if done.contains(node) {
            return false;
        }
        if !visiting.insert(node) {
            return true;
        }
        if edges
            .get(node)
            .is_some_and(|next| next.iter().any(|n| visit(n, edges, visiting, done)))
        {
            return true;
        }
        visiting.remove(node);
        done.insert(node);
        false
    }
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    edges
        .keys()
        .any(|node| visit(node, edges, &mut visiting, &mut done))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_is_rejected_even_across_multiple_buses() {
        let mut edges = HashMap::from([
            ("speaker".into(), vec!["mix".into()]),
            ("mix".into(), vec!["rooms".into()]),
        ]);
        assert!(!has_cycle(&edges));
        edges.insert("rooms".into(), vec!["speaker".into()]);
        assert!(has_cycle(&edges));
    }

    #[tokio::test]
    async fn opening_the_patchbay_persists_headless_opt_in_without_a_route() {
        let directory =
            std::env::temp_dir().join(format!("on-air-patchbay-{}", session::new_stream_nonce()));
        let path = directory.join("patchbay.json");
        let mut state = CoreState::new_mock().await;
        state.patchbay = std::sync::Arc::new(Patchbay::new(Some(path.clone()), true));
        state.patchbay.refresh(&state).await;
        assert!(path.exists());
        assert!(Patchbay::new(Some(path), true)
            .activated
            .load(Ordering::Acquire));
        state.shutdown().await;
        std::fs::remove_dir_all(directory).unwrap();
    }
}
