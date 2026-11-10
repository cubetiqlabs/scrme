use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::capture::Frame;
use crate::output::trim_memory;
use crate::platform::activate_app;
use crate::state;

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

/// Show the webview selection overlay for a frozen frame (non-macOS, or
/// `SCRME_WEBVIEW_OVERLAY=1`). Returns false when the window can't be built.
pub(crate) fn show_web_overlay(app: &AppHandle, frame: Frame, origin: (f64, f64)) -> bool {
    let (w, h) = frame.size();
    state::set_frozen(frame);
    let Some(win) = app
        .get_webview_window("overlay")
        .or_else(|| build_overlay(app).ok())
    else {
        state::clear_frozen();
        return false;
    };
    let _ = win.set_position(PhysicalPosition::new(origin.0, origin.1));
    let _ = win.set_size(PhysicalSize::new(w, h));
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.eval("window.__arm&&window.__arm()");
    true
}

pub(crate) fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        raise(app, &w);
        return;
    }
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Preferences")
        .inner_size(440.0, 770.0)
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
