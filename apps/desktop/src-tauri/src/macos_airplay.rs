use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_av_kit::AVRoutePickerView;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;

thread_local! {
    static PICKER_WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

pub fn present_route_picker() -> Result<String, String> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "AVRoutePickerView must be created on the main thread".to_string())?;
    if PICKER_WINDOW.with(|window| {
        if let Some(window) = window.borrow().as_ref() {
            window.makeKeyAndOrderFront(None);
            true
        } else {
            false
        }
    }) {
        return Ok("avroute-picker".into());
    }
    let frame = NSRect::new(NSPoint::new(200.0, 200.0), NSSize::new(240.0, 80.0));
    let picker_frame = NSRect::new(NSPoint::new(80.0, 20.0), NSSize::new(80.0, 40.0));
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // Retained owns this window through shutdown. AppKit's default release on
    // close would leave a dangling pointer and release it again in the TLS drop.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("AirPlay"));
    let picker =
        unsafe { AVRoutePickerView::initWithFrame(AVRoutePickerView::alloc(mtm), picker_frame) };
    let content = window
        .contentView()
        .ok_or_else(|| "window has no content view".to_string())?;
    content.addSubview(&picker);
    window.makeKeyAndOrderFront(None);
    if content.subviews().is_empty() {
        return Err("AVRoutePickerView was not added to the window".into());
    }
    PICKER_WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
    Ok("avroute-picker".into())
}
