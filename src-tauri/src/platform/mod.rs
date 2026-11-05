//! Platform-specific UI that must not be a webview (a full-screen retina WKWebView cost
//! 75-105 MB plus helper processes). Each item has a webview fallback elsewhere.

#[cfg(target_os = "macos")]
pub(crate) mod overlay_macos;
#[cfg(target_os = "macos")]
pub(crate) mod toast_macos;
