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
//! Only questions with a caller live here. The OS identity itself looked like
//! an obvious thing to export, but nothing reads it yet and a constant with no
//! production caller is dead code wearing a design hat — it comes back when a
//! client actually needs to ask which OS this is.
//!
//! See `docs/DESIGN-cross-platform.md` for the boundary these enforce: arrows
//! cross into OS bindings only from `packages/core`, never from a client.

/// Repairing the firewall shells out to `pkexec` and `journalctl`, which only
/// exist in the Linux story. Elsewhere the warning is shown but not repaired.
pub const FIREWALL_REPAIR: bool = cfg!(target_os = "linux");

/// The patchbay drives PulseAudio/PipeWire buses to mix and fan out locally.
pub const PATCHBAY: bool = cfg!(target_os = "linux");

#[cfg(test)]
mod tests {
    use super::*;

    /// The constants must agree with the OS they claim to describe. This is
    /// the one place where a wrong answer is visible without hardware.
    #[test]
    fn capability_matches_the_target_os() {
        let linux = cfg!(target_os = "linux");
        assert_eq!(FIREWALL_REPAIR, linux);
        assert_eq!(PATCHBAY, linux);
    }
}
