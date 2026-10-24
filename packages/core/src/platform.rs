//! Platform capability, in one place.
//!
//! The core owns the support decisions that have production callers:
//! firewall repair and the Linux patchbay. Other OS-specific behavior remains
//! next to the binding it selects, where a `cfg` branch is the implementation
//! boundary rather than a duplicated support matrix.
//!
//! These gates are pure and allocation-free, so the core can expose their
//! result in a wire snapshot or reject an operation before invoking an OS
//! helper. They are `const` and fold away at compile time.
//!
//! Only questions with a caller live here. The OS identity itself is not
//! exported: a public enum with no production caller would be dead code rather
//! than a capability boundary. Add it only when a client needs that answer.
//!
//! See `docs/DESIGN-cross-platform.md` for the binding rules: OS helpers are
//! reached only from core modules, never directly from a client.

/// Repairing the firewall shells out to `pkexec` and `journalctl`, which only
/// exist in the Linux story. Elsewhere the warning is shown but not repaired.
pub(crate) const FIREWALL_REPAIR: bool = cfg!(target_os = "linux");

/// The patchbay drives PulseAudio/PipeWire buses to mix and fan out locally.
pub(crate) const PATCHBAY: bool = cfg!(target_os = "linux");

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
