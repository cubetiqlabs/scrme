use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::output::trim_memory;
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

/// Generation counter so only the newest "destroy the idle editor" timer acts.
static EDITOR_GEN: AtomicU32 = AtomicU32::new(0);
/// How long a hidden editor window is kept warm for an instant re-open.
const EDITOR_KEEP_SECS: u64 = 90;

fn build_editor(app: &AppHandle) -> Option<WebviewWindow> {
    // Frameless like Preferences: the editor draws its own glass toolbar (drag region + close).
    WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("editor.html".into()))
        .title("scrme")
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .min_inner_size(780.0, 400.0)
        .visible(false)
        .build()
        .ok()
}

/// Destroy the editor window if it is still hidden after `EDITOR_KEEP_SECS` (frees the webview).
fn schedule_editor_cleanup(app: &AppHandle) {
    let id = EDITOR_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(EDITOR_KEEP_SECS));
        if EDITOR_GEN.load(Ordering::SeqCst) != id {
            return;
        }
        if let Some(w) = app.get_webview_window("editor")
            && !w.is_visible().unwrap_or(false)
        {
            let _ = w.destroy();
            trim_memory();
        }
    });
}

/// Build the (hidden) editor window ahead of time, e.g. while a preview is on screen,
/// so the Edit button opens instantly. Unused, it is destroyed again after a while.
pub(crate) fn prewarm_editor(app: &AppHandle) {
    if app.get_webview_window("editor").is_none() && build_editor(app).is_some() {
        schedule_editor_cleanup(app);
    }
}

/// Hide the editor, release its image, and keep the webview warm for a short while.
pub(crate) fn park_editor(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("editor") {
        let _ = w.eval("window.__reset&&window.__reset()");
        let _ = w.hide();
        schedule_editor_cleanup(app);
    }
}

pub(crate) fn open_editor(app: &AppHandle, w: u32, h: u32) {
    EDITOR_GEN.fetch_add(1, Ordering::SeqCst); // cancel a pending cleanup
    let Some(win) = app
        .get_webview_window("editor")
        .or_else(|| build_editor(app))
    else {
        return;
    };
    // Image in points + toolbar + breathing room, capped to a sensible window.
    let s = win.scale_factor().unwrap_or(1.0);
    let lw = (w as f64 / s + 120.0).clamp(780.0, 1400.0);
    let lh = (h as f64 / s + 40.0 + 120.0).clamp(400.0, 900.0);
    let _ = win.set_size(tauri::LogicalSize::new(lw, lh));
    let _ = win.center();
    raise(app, &win);
    // A page that is already running loads the new image itself; one still starting up reloads.
    let _ = win.eval("window.__open ? window.__open() : location.reload()");
}
