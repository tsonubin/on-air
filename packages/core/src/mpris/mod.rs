//! The D-Bus MPRIS bridge: external media consumers (Omarchy's media
//! widget, Waybar's `mpris` module, `playerctl`) see and control on-air
//! with no further wiring. The interface contract is docs/omarchy.md
//! "Media"; [`model`] maps every part of it to the core's state and back,
//! and [`bridge`] owns the session-bus plumbing on Linux.
//!
//! The activation model — in-process inside `on-air-core`, gated on
//! demand — the dependency choice and the cost analysis are documented on
//! [`bridge`]. In short: no separate daemon, no polling; the bridge holds
//! a bus connection only while there is a disc in the drive or an active
//! output to show.

pub mod model;

#[cfg(target_os = "linux")]
mod bridge;

#[cfg(target_os = "linux")]
pub use bridge::{spawn, spawn_on, OBJECT_PATH, SERVICE_NAME};
