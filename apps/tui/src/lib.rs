//! on-air terminal UI: a loopback client of the on-air-core service.
//!
//! Nothing here owns audio. The binary is a view over the core's HTTP and
//! WebSocket control plane plus a few subcommands for Omarchy's bar and
//! menu; closing it never stops streaming.

pub mod api;
pub mod autostart;
pub mod cli;
pub mod client;
pub mod error_copy;
pub mod handoff;
pub mod keys;
pub mod labels;
pub mod model;
pub mod service;
pub mod theme;
pub mod update;
pub mod view;
