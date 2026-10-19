//! Platform capability, in one place.
//!
//! `cfg!(target_os = ...)` was being re-derived at each call site to answer
//! the same question — "is this feature available here?" — in `api/firewall.rs`,
//! `patchbay/pulse.rs`, `patchbay/mod.rs`, `pipeline/capture.rs` and the TUI's
//! settings handoff. That scattered the answer across half a dozen files and a
//! docs table that could drift from both.
//!
//! These are the single source. They are **pure**: no I/O and no allocation, so
//! a client can decide whether to grey a control out before asking the core
//! anything. Everything is `const` and folds away at compile time.
//!
//! See `docs/DESIGN-cross-platform.md` for the boundary these enforce: arrows
//! cross into OS bindings only from `packages/core`, never from a client.

/// The OS this build targets. `Other` covers anything unrecognised, where
/// nothing platform-specific is assumed to work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Macos,
    Windows,
    Other,
}

/// The OS this binary was compiled for. Constant-folded away at runtime.
pub const OS: Os = if cfg!(target_os = "linux") {
    Os::Linux
} else if cfg!(target_os = "macos") {
    Os::Macos
} else if cfg!(target_os = "windows") {
    Os::Windows
} else {
    Os::Other
};

/// Repairing the firewall shells out to `pkexec` and `journalctl`, which only
/// exist in the Linux story. Elsewhere the warning is shown but not repaired.
pub const FIREWALL_REPAIR: bool = cfg!(target_os = "linux");

/// The patchbay drives PulseAudio/PipeWire buses to mix and fan out locally.
pub const PATCHBAY: bool = cfg!(target_os = "linux");

/// System-audio capture reads a Pulse monitor source. Elsewhere capture comes
/// from a device the user picks rather than from what the system is playing.
pub const PULSE_MONITOR: bool = cfg!(target_os = "linux");

#[cfg(test)]
mod tests {
    use super::*;

    /// The constants must agree with the OS they claim to describe. This is
    /// the one place where a wrong answer is visible without hardware.
    #[test]
    fn capability_matches_the_target_os() {
        let linux = OS == Os::Linux;
        assert_eq!(FIREWALL_REPAIR, linux);
        assert_eq!(PATCHBAY, linux);
        assert_eq!(PULSE_MONITOR, linux);
    }

    #[test]
    fn the_os_is_one_of_the_four() {
        assert!(matches!(OS, Os::Linux | Os::Macos | Os::Windows | Os::Other));
    }
}
