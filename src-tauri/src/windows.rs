use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub(crate) fn build_overlay(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .build()
}

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

/// Bring a window to the front of everything: activate the app, then show + focus,
/// all on the main thread (AppKit requirement).
pub(crate) fn raise(app: &AppHandle, w: &WebviewWindow) {
    let w = w.clone();
    let _ = app.run_on_main_thread(move || {
        activate_app();
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    });
}

pub(crate) fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        raise(app, &w);
        return;
    }
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Preferences")
        .inner_size(440.0, 690.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .resizable(false)
        .focused(true)
        .center()
        .build();
    if let Ok(w) = built {
        raise(app, &w);
    }
}

pub(crate) fn open_editor(app: &AppHandle, w: u32, h: u32) {
    // Frameless like Preferences: the editor draws its own glass toolbar (drag region + close).
    let existing = app.get_webview_window("editor");
    let reuse = existing.is_some();
    let Some(win) = existing.or_else(|| {
        WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("editor.html".into()))
            .title("scrme")
            .decorations(false)
            .transparent(true)
            .shadow(true)
            .min_inner_size(900.0, 420.0)
            .visible(false)
            .build()
            .ok()
    }) else {
        return;
    };
    // Image in points + toolbar (52) + breathing room, capped to a sensible window.
    let s = win.scale_factor().unwrap_or(1.0);
    let lw = (w as f64 / s + 120.0).clamp(900.0, 1400.0);
    let lh = (h as f64 / s + 52.0 + 120.0).clamp(420.0, 900.0);
    let _ = win.set_size(tauri::LogicalSize::new(lw, lh));
    let _ = win.center();
    raise(app, &win);
    // A page that is already open must pick up the new image; a new one loads it itself.
    if reuse {
        let _ = win.eval("location.reload()");
    }
}
