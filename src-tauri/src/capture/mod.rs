mod frame;
pub(crate) mod geometry;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::Arc;
use std::time::Instant;

use crate::host::{self, Host};
use crate::output::{self, trim_memory};
use crate::preview;
use crate::state;
use crate::toast;

pub(crate) use frame::Frame;

/// Crop rect in physical pixels, clamped to the image. None if empty.
pub(crate) fn clamp_rect(
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    iw: u32,
    ih: u32,
) -> Option<(u32, u32, u32, u32)> {
    let x0 = x.clamp(0, iw as i64);
    let y0 = y.clamp(0, ih as i64);
    let x1 = (x + w).clamp(0, iw as i64);
    let y1 = (y + h).clamp(0, ih as i64);
    (x1 > x0 && y1 > y0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

pub(crate) fn primary_monitor() -> Option<xcap::Monitor> {
    xcap::Monitor::all()
        .ok()?
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
}

/// Freeze the primary monitor. Returns the frame and the monitor's origin in physical pixels.
/// macOS keeps a CGImage (cheap); anything else, or a failure, falls back to xcap's RGBA copy.
pub(crate) fn freeze_primary() -> Option<(Frame, (f64, f64))> {
    let mon = primary_monitor()?;
    let scale = mon.scale_factor().unwrap_or(1.0) as f64;
    let origin = (
        mon.x().unwrap_or(0) as f64 * scale,
        mon.y().unwrap_or(0) as f64 * scale,
    );
    #[cfg(target_os = "macos")]
    if let Some(cg) = mon.id().ok().and_then(macos::freeze) {
        return Some((Frame::Cg(cg), origin));
    }
    mon.capture_image()
        .ok()
        .map(|img| (Frame::Rgba(img), origin))
}

/// Freeze the primary monitor, then show the overlay (native on macOS).
pub(crate) fn start_area_capture() {
    let host = host::host();
    let t = Instant::now();
    // ponytail: primary monitor only; one overlay per monitor for multi-display.
    let Some((frame, origin)) = freeze_primary() else {
        eprintln!("capture failed (Screen Recording permission?)");
        toast::err(&host, "Capture failed");
        return;
    };

    // The engine's own overlay (webview on Tauri) takes over when the native one can't run.
    #[cfg(target_os = "macos")]
    match native_overlay(frame, &host) {
        Ok(()) => eprintln!("hotkey→native overlay {:?}", t.elapsed()),
        Err(frame) => {
            if host.show_web_overlay(frame, origin) {
                eprintln!("hotkey→overlay {:?}", t.elapsed());
            } else {
                toast::err(&host, "Capture failed");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if !host.show_web_overlay(frame, origin) {
            toast::err(&host, "Capture failed");
        }
    }
}

/// Show the native overlay for a CGImage frame. Gives the frame back when it can't
/// (other pixel source, or `SCRME_WEBVIEW_OVERLAY=1`) so the engine's overlay takes over.
#[cfg(target_os = "macos")]
fn native_overlay(frame: Frame, host: &Arc<dyn Host>) -> Result<(), Frame> {
    use crate::platform::overlay_macos::{self, SendImage};

    let Frame::Cg(cg) = &frame else {
        return Err(frame);
    };
    // Only the Tauri engine has a webview overlay to fall back to.
    #[cfg(feature = "tauri-app")]
    if std::env::var_os("SCRME_WEBVIEW_OVERLAY").is_some() {
        return Err(frame);
    }
    let image = SendImage(cg.image());
    state::set_frozen(frame);

    let shown = host.on_main(Box::new(move || {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        crate::platform::activate_app(); // an Accessory app must be active to receive key events (Esc)
        overlay_macos::show(mtm, &image.0, move |sel| {
            // Already on the main thread: hand the heavy work (crop, clipboard) to a worker.
            std::thread::spawn(move || match sel {
                Some(s) => commit_selection(s.x, s.y, s.w, s.h, s.edit),
                None => {
                    state::clear_frozen();
                    trim_memory();
                }
            });
        });
    }));
    if !shown {
        state::clear_frozen();
    }
    Ok(())
}

/// Crop the frozen frame to the selection, then copy it (or open the editor).
/// Shared by the engine overlay command and the native overlay.
pub(crate) fn commit_selection(x: i64, y: i64, w: i64, h: i64, edit: bool) {
    let t = Instant::now();
    let Some(frame) = state::take_frozen() else {
        return;
    };
    let (iw, ih) = frame.size();
    let Some((cx, cy, cw, ch)) = clamp_rect(x, y, w, h, iw, ih) else {
        return;
    };
    let Some(crop) = frame.crop(cx, cy, cw, ch) else {
        return;
    };
    drop(frame); // release the screenshot before the (slower) clipboard/editor work
    if edit {
        preview::open_editor_with(crop);
    } else {
        output::finish(crop);
    }
    eprintln!("commit {:?}", t.elapsed());
}

pub(crate) fn capture_full() {
    match freeze_primary().and_then(|(frame, _)| frame.into_rgba()) {
        Some(img) => output::finish(img),
        None => toast::err(&host::host(), "Capture failed"),
    }
}

/// Focused window of the frontmost app (we are an Accessory app, so never ourselves).
pub(crate) fn capture_window() {
    let win = xcap::Window::all().ok().and_then(|w| {
        w.into_iter()
            .find(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(true))
    });
    match win.and_then(|w| w.capture_image().ok()) {
        Some(img) => output::finish(img),
        None => toast::err(&host::host(), "No window found"),
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_rect;

    #[test]
    fn clamp() {
        assert_eq!(clamp_rect(10, 10, 50, 50, 100, 100), Some((10, 10, 50, 50)));
        assert_eq!(clamp_rect(-5, -5, 20, 20, 100, 100), Some((0, 0, 15, 15)));
        assert_eq!(clamp_rect(90, 90, 50, 50, 100, 100), Some((90, 90, 10, 10)));
        assert_eq!(clamp_rect(200, 0, 5, 5, 100, 100), None);
    }
}
