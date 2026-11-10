//! The seam between the engine-agnostic core and whichever UI toolkit is running it.
//!
//! `capture`, `output`, `store` and `preview` never mention Tauri or GPUI: they hand the
//! small set of platform jobs below to the installed [`Host`]. This is what lets the same
//! capture path run under Tauri today and under GPUI (issue #63) tomorrow.

use std::sync::{Arc, OnceLock};

use image::RgbaImage;

use crate::toast::Kind;

pub trait Host: Send + Sync + 'static {
    /// Run `f` on the AppKit main thread (windows, pasteboard and Core Animation need it).
    /// Returns false when the engine is already shutting down and cannot dispatch.
    fn on_main(&self, f: Box<dyn FnOnce() + Send>) -> bool;
    /// Engine-specific toast fallback. The native macOS pill is tried first (see `toast`).
    fn toast(&self, kind: Kind, text: &str);
    /// Put an image on the system clipboard.
    fn copy_image(&self, img: &RgbaImage);
    /// Show (or focus) the annotation editor. The image itself is already in
    /// [`state::take_pending`](crate::state::take_pending); only its size is passed here.
    fn open_editor(&self, w: u32, h: u32);
    /// Build the editor window ahead of time so "Edit" opens instantly. Optional.
    fn prewarm_editor(&self) {}

    /// Fallback selection UI for engines/platforms without the native overlay (Tauri's
    /// webview). Returns false when the engine has nothing to offer.
    fn show_web_overlay(&self, frame: crate::capture::Frame, origin: (f64, f64)) -> bool {
        let _ = (frame, origin);
        false
    }
}

static HOST: OnceLock<Arc<dyn Host>> = OnceLock::new();

/// Install the engine's host. Called once, at startup, before any capture can happen.
pub fn install(host: Arc<dyn Host>) {
    let _ = HOST.set(host);
}

/// For paths that only run after startup (hotkeys, tray, CLI).
pub fn host() -> Arc<dyn Host> {
    HOST.get()
        .expect("host is installed during startup")
        .clone()
}
