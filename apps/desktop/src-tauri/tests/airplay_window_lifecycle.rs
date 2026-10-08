//! AppKit regression for the AirPlay route-picker window: open, close and
//! reopen must reuse one Rust-owned `NSWindow`, and AppKit must not release
//! it on close.
//!
//! This is a `harness = false` test so it can own the main thread. It only
//! exercises anything on macOS; elsewhere it prints "skipped: macOS only" and
//! exits 0, so a green run on Linux or Windows does not mean the lifecycle
//! was checked.

#[cfg(target_os = "macos")]
mod macos_airplay {
    include!("../src/macos_airplay.rs");

    pub fn assert_window_lifecycle() {
        present_route_picker().unwrap();
        let original = PICKER_WINDOW.with(|slot| slot.borrow().as_ref().unwrap().clone());
        for _ in 0..3 {
            PICKER_WINDOW.with(|slot| {
                let slot = slot.borrow();
                let window = slot.as_ref().unwrap();
                assert!(
                    !window.isReleasedWhenClosed(),
                    "Rust owns the window; AppKit must not release it on close"
                );
                window.close();
            });
            present_route_picker().unwrap();
            PICKER_WINDOW.with(|slot| {
                let slot = slot.borrow();
                let window = slot.as_ref().unwrap();
                assert!(
                    std::ptr::eq(&**window, &*original),
                    "reopening must reuse the window"
                );
                assert!(window
                    .contentView()
                    .is_some_and(|view| !view.subviews().is_empty()));
            });
        }
        PICKER_WINDOW.with(|slot| slot.borrow_mut().take());
        drop(original);
        // Leave one closed, owned window for TLS teardown at process exit, matching
        // the user's shutdown stack rather than testing only explicit cleanup.
        present_route_picker().unwrap();
        PICKER_WINDOW.with(|slot| slot.borrow().as_ref().unwrap().close());
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {
        let main = objc2::MainThreadMarker::new().expect("AppKit regression runs on main thread");
        let _app = objc2_app_kit::NSApplication::sharedApplication(main);
        macos_airplay::assert_window_lifecycle();
        println!("AirPlay open / close / reopen / release passed");
    });
    #[cfg(not(target_os = "macos"))]
    println!("skipped: macOS only (airplay_window_lifecycle needs AppKit)");
}
