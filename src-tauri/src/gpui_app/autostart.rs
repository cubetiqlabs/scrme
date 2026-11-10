//! Launch at login, without a plugin: the same LaunchAgent file the Tauri build wrote
//! (`~/Library/LaunchAgents/scrme.plist`), so switching engines does not create a second one.

use std::path::PathBuf;

/// Label and file name of the agent (the tauri-plugin-autostart default).
const LABEL: &str = "scrme";

#[cfg(target_os = "macos")]
fn plist_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("no home directory")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Make the login item match `want`. Idempotent: called at startup and from Preferences.
#[cfg(target_os = "macos")]
pub(crate) fn set(want: bool) -> Result<(), String> {
    let path = plist_path()?;
    if !want {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = path.parent().ok_or("bad launch agent path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>AssociatedBundleIdentifiers</key>
  <array></array>
  <key>ProgramArguments</key>
  <array><string>{}</string></array>
  <key>RunAtLoad</key>
  <true/>
  </dict>
</plist>
"#,
        xml_escape(&exe.display().to_string())
    );
    // Already points at this executable: leave the file (and its mtime) alone.
    if matches!(std::fs::read_to_string(&path), Ok(existing) if existing == body) {
        return Ok(());
    }
    std::fs::write(&path, body).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn set(_want: bool) -> Result<(), String> {
    Err("launch at login is only implemented on macOS".into())
}
