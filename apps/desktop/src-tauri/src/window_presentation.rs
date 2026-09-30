//! Whether a login launch may hide the window.
//!
//! Hiding the window before the webview finishes its first load leaves the
//! page blank when the tray later shows it. The window stays visible until
//! that load finishes, unless the user has already asked to see it.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub struct WindowPresentation {
    pending_autohide: AtomicBool,
}

impl WindowPresentation {
    pub fn new(start_hidden: bool) -> Self {
        Self {
            pending_autohide: AtomicBool::new(start_hidden),
        }
    }

    pub fn request_show(&self) {
        self.pending_autohide.store(false, Ordering::Release);
    }

    pub fn should_hide_after_load(&self) -> bool {
        self.pending_autohide.load(Ordering::Acquire)
    }

    /// Claims the pending hide. A later call returns false, including after
    /// the user has asked to see the window, so a deferred hide cannot cover it.
    pub fn take_autohide(&self) -> bool {
        self.pending_autohide.swap(false, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_launch_hides_only_before_the_user_asks_for_the_window() {
        let presentation = WindowPresentation::new(true);
        assert!(presentation.should_hide_after_load());
        presentation.request_show();
        assert!(!presentation.should_hide_after_load());
        assert!(!presentation.take_autohide());
    }

    #[test]
    fn the_hide_can_be_claimed_only_once() {
        let presentation = WindowPresentation::new(true);
        assert!(presentation.take_autohide());
        assert!(!presentation.take_autohide());
    }

    #[test]
    fn a_normal_launch_never_autohides() {
        let presentation = WindowPresentation::new(false);
        assert!(!presentation.should_hide_after_load());
        assert!(!presentation.take_autohide());
    }
}
