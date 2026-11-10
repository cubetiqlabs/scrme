//! Platform-specific UI that must not be a webview (a full-screen retina WKWebView cost
//! 75-105 MB plus helper processes). Each item has a webview fallback elsewhere.

#[cfg(target_os = "macos")]
pub(crate) mod overlay_macos;
#[cfg(target_os = "macos")]
pub(crate) mod pin_macos;
#[cfg(target_os = "macos")]
pub(crate) mod toast_macos;

/// Make this (Accessory-policy) app the active app. Without it macOS ignores
/// `show()`/`set_focus()` and the window opens behind whatever is frontmost.
#[cfg(target_os = "macos")]
pub(crate) fn activate_app() {
    use objc2::runtime::NSObjectProtocol;
    use objc2::{MainThreadMarker, msg_send, sel};
    use objc2_app_kit::NSApplication;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // `activate` (macOS 14+) is the cooperative API; older systems need the legacy call.
    if app.respondsToSelector(sel!(activate)) {
        // SAFETY: `-[NSApplication activate]` takes no arguments and returns void.
        unsafe { msg_send![&*app, activate] }
    } else {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn activate_app() {}
