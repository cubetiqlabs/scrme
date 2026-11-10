//! Success/failure toast. macOS draws a native Core Animation pill (no webview, no
//! helper process); other engines can supply their own fallback through
//! [`Host::toast`](crate::host::Host::toast).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::host::Host;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Ok,
    Err,
}

pub(crate) fn ok(host: &Arc<dyn Host>, text: &str) {
    show(host, Kind::Ok, text);
}

pub(crate) fn err(host: &Arc<dyn Host>, text: &str) {
    show(host, Kind::Err, text);
}

/// Convenience for call sites that don't already hold the host.
pub(crate) fn ok_now(text: &str) {
    ok(&crate::host::host(), text);
}

pub(crate) fn err_now(text: &str) {
    err(&crate::host::host(), text);
}

pub(crate) fn show(host: &Arc<dyn Host>, kind: Kind, text: &str) {
    #[cfg(target_os = "macos")]
    if native(host, kind, text) {
        return;
    }
    host.toast(kind, text);
}

/// How long each kind stays on screen.
fn millis(kind: Kind) -> u64 {
    match kind {
        Kind::Ok => 1500,
        Kind::Err => 2100,
    }
}

/// Native macOS toast (no webview). Returns false when it can't run, so the engine's
/// fallback takes over.
#[cfg(target_os = "macos")]
fn native(host: &Arc<dyn Host>, kind: Kind, text: &str) -> bool {
    use crate::platform::toast_macos;

    // Only the Tauri engine has a webview toast to fall back to.
    #[cfg(feature = "tauri-app")]
    if std::env::var_os("SCRME_WEBVIEW_TOAST").is_some() {
        return false;
    }
    let id = TOAST_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let text = text.to_string();
    let shown = host.on_main(Box::new(move || {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            let kind = match kind {
                Kind::Ok => toast_macos::Kind::Ok,
                Kind::Err => toast_macos::Kind::Err,
            };
            toast_macos::show(mtm, kind, &text);
        }
    }));
    if !shown {
        return false;
    }
    let host = host.clone();
    let ms = millis(kind);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if TOAST_GEN.load(Ordering::SeqCst) == id {
            host.on_main(Box::new(toast_macos::hide));
        }
    });
    true
}

/// Generation counter: a newer toast keeps the window, older hide timers stand down.
static TOAST_GEN: AtomicU32 = AtomicU32::new(0);

#[cfg(feature = "tauri-app")]
mod webview {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use tauri::{
        AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    };

    use super::{Kind, TOAST_GEN};

    pub(crate) const TOAST_W: f64 = 170.0;
    pub(crate) const TOAST_H: f64 = 44.0;

    /// Transparent, click-through, never-focused pill with the success animation.
    /// A freshly built page isn't loaded yet, so a first message rides in the URL.
    pub(crate) fn build(
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
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    /// Webview fallback (non-macOS, or `SCRME_WEBVIEW_TOAST=1` on macOS).
    pub(crate) fn show(app: &AppHandle, kind: Kind, text: &str) {
        // Normally pre-built at startup; if a toast beats the warm-up, build it with the message in the URL.
        let existing = app.get_webview_window("toast");
        let fresh = existing.is_none();
        let Some(w) = existing.or_else(|| build(app, Some((kind_name(kind), text))).ok()) else {
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
            let _ = w.eval(format!(
                "window.__play&&window.__play({text:?},{:?})",
                kind_name(kind)
            ));
        }
        let ms = super::millis(kind);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            if TOAST_GEN.load(Ordering::SeqCst) != id {
                return; // a newer toast owns the window now
            }
            let _ = w.hide();
        });
    }

    fn kind_name(kind: Kind) -> &'static str {
        match kind {
            Kind::Ok => "ok",
            Kind::Err => "err",
        }
    }
}

#[cfg(feature = "tauri-app")]
#[cfg_attr(target_os = "macos", allow(unused_imports))]
pub(crate) use webview::{build as build_webview, show as show_webview};

#[cfg(all(test, feature = "tauri-app"))]
mod tests {
    use super::webview::pct;

    #[test]
    fn pct_encodes_query_values() {
        assert_eq!(pct("Copied & saved"), "Copied%20%26%20saved");
        assert_eq!(pct("Updating…"), "Updating%E2%80%A6");
        assert_eq!(pct("ok"), "ok");
    }
}
