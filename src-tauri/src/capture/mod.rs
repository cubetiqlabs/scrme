mod frame;
#[cfg(target_os = "macos")]
mod macos;

use std::time::Instant;

use crate::{output::*, state::*, toast::*, windows::*};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

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
pub(crate) fn freeze_primary() -> Option<(Frame, PhysicalPosition<f64>)> {
    let mon = primary_monitor()?;
    let scale = mon.scale_factor().unwrap_or(1.0) as f64;
    let origin = PhysicalPosition::new(
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

/// Freeze the primary monitor, then show the (pre-built) transparent overlay.
pub(crate) fn start_area_capture(app: &AppHandle, edit: bool) {
    let t = Instant::now();
    // ponytail: primary monitor only; one overlay per monitor for multi-display.
    let Some((frame, pos)) = freeze_primary() else {
        eprintln!("capture failed (Screen Recording permission?)");
        show_toast_err(app, "Capture failed");
        return;
    };
    app.state::<EditMode>()
        .0
        .store(edit, std::sync::atomic::Ordering::SeqCst);
    let (w, h) = frame.size();
    *app.state::<Frozen>().0.lock().unwrap() = Some(frame);

    let Some(win) = app
        .get_webview_window("overlay")
        .or_else(|| build_overlay(app).ok())
    else {
        return;
    };
    let _ = win.set_position(pos);
    let _ = win.set_size(PhysicalSize::new(w, h));
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.eval("window.__arm&&window.__arm()");
    eprintln!("hotkey→overlay {:?}", t.elapsed());
}

pub(crate) fn capture_full(app: &AppHandle) {
    match freeze_primary().and_then(|(frame, _)| frame.into_rgba()) {
        Some(img) => finish(app, img),
        None => show_toast_err(app, "Capture failed"),
    }
}

/// Focused window of the frontmost app (we are an Accessory app, so never ourselves).
pub(crate) fn capture_window(app: &AppHandle) {
    let win = xcap::Window::all().ok().and_then(|w| {
        w.into_iter()
            .find(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(true))
    });
    match win.and_then(|w| w.capture_image().ok()) {
        Some(img) => finish(app, img),
        None => show_toast_err(app, "No window found"),
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
