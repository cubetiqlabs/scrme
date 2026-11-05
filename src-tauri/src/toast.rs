use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

/// Transparent, click-through, never-focused pill with the success animation.
/// A freshly built page isn't loaded yet, so a first message rides in the URL.
pub(crate) fn build_toast(
    app: &AppHandle,
    first: Option<(&str, &str)>,
) -> tauri::Result<WebviewWindow> {
    let url = match first {
        Some((kind, text)) => format!("toast.html?k={}&t={}", pct(kind), pct(text)),
        None => "toast.html".into(),
    };
    let w = WebviewWindowBuilder::new(app, "toast", WebviewUrl::App(url.into()))
        .inner_size(TOAST_W, TOAST_H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    w.set_ignore_cursor_events(true)?;
    Ok(w)
}

/// Minimal percent-encoding for a URL query value.
pub(crate) fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub(crate) const TOAST_W: f64 = 170.0;
pub(crate) const TOAST_H: f64 = 44.0;
pub(crate) static TOAST_GEN: AtomicU32 = AtomicU32::new(0);

pub(crate) fn show_toast(app: &AppHandle, text: &str) {
    show_toast_kind(app, "ok", text);
}

/// Sad, red variant for failures.
pub(crate) fn show_toast_err(app: &AppHandle, text: &str) {
    show_toast_kind(app, "err", text);
}

pub(crate) fn show_toast_kind(app: &AppHandle, kind: &str, text: &str) {
    // Normally pre-built at startup; if a toast beats the warm-up, build it with the message in the URL.
    let existing = app.get_webview_window("toast");
    let fresh = existing.is_none();
    let Some(w) = existing.or_else(|| build_toast(app, Some((kind, text))).ok()) else {
        return;
    };
    if let Ok(Some(m)) = w.primary_monitor() {
        let (mp, ms, s) = (m.position(), m.size(), m.scale_factor());
        let x = mp.x as f64 + (ms.width as f64 - TOAST_W * s) / 2.0;
        let y = mp.y as f64 + ms.height as f64 - (TOAST_H + 100.0) * s;
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let id = TOAST_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = w.show();
    if !fresh {
        let _ = w.eval(format!("window.__play&&window.__play({text:?},{kind:?})"));
    }
    let ms = if kind == "err" { 2100 } else { 1500 };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if TOAST_GEN.load(Ordering::SeqCst) != id {
            return; // a newer toast owns the window now
        }
        let _ = w.hide();
    });
}

#[cfg(test)]
mod tests {
    use super::pct;

    #[test]
    fn pct_encodes_query_values() {
        assert_eq!(pct("Copied & saved"), "Copied%20%26%20saved");
        assert_eq!(pct("Updating…"), "Updating%E2%80%A6");
        assert_eq!(pct("ok"), "ok");
    }
}
