use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) area: String,
    pub(crate) full: String,
    pub(crate) window: String,
    /// Also write a PNG to ~/Pictures/scrme on every quick capture.
    pub(crate) autosave: bool,
    /// Show the success toast.
    pub(crate) animation: bool,
    /// Check for updates on launch and every few hours; download in the background.
    pub(crate) auto_update: bool,
    /// Start scrme (in the menu bar) when you log in.
    pub(crate) launch_at_login: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            area: "Cmd+Shift+2".into(),
            full: "Cmd+Shift+1".into(),
            window: "Cmd+Shift+3".into(),
            autosave: false,
            animation: true,
            auto_update: true,
            launch_at_login: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Keys {
    pub(crate) area: Shortcut,
    pub(crate) full: Shortcut,
    pub(crate) window: Shortcut,
}
impl Keys {
    pub(crate) fn parse(c: &Config) -> Result<Self, String> {
        let p = |name: &str, v: &str| v.parse::<Shortcut>().map_err(|e| format!("{name}: {e}"));
        let k = Self {
            area: p("Area", &c.area)?,
            full: p("Screen", &c.full)?,
            window: p("Window", &c.window)?,
        };
        let all = k.all();
        if all.iter().enumerate().any(|(i, a)| all[..i].contains(a)) {
            return Err("Each action needs a different shortcut".into());
        }
        Ok(k)
    }

    pub(crate) fn all(&self) -> [Shortcut; 3] {
        [self.area, self.full, self.window]
    }
}

pub(crate) struct Settings {
    pub(crate) cfg: Mutex<Config>,
    pub(crate) keys: Mutex<Keys>,
}

pub(crate) fn config_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("config.json"))
}

pub(crate) fn load_config(app: &AppHandle) -> Config {
    config_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub(crate) fn write_config(app: &AppHandle, cfg: &Config) -> std::io::Result<()> {
    let p = config_path(app).ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(p, serde_json::to_string_pretty(cfg).unwrap())
}

/// Load config, register the global hotkeys and publish `Settings` as app state.
/// Make the login item match the setting (a LaunchAgent on macOS). Cheap no-op when it already does.
pub(crate) fn apply_autostart(app: &AppHandle, want: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let is_on = launcher.is_enabled().map_err(|e| e.to_string())?;
    match (want, is_on) {
        (true, false) => launcher.enable().map_err(|e| e.to_string()),
        (false, true) => launcher.disable().map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

pub(crate) fn init(app: &tauri::App) {
    let handle = app.handle();
    let cfg = load_config(handle);
    let keys = Keys::parse(&cfg).unwrap_or_else(|_| Keys::parse(&Config::default()).unwrap());
    if config_path(handle).is_some_and(|p| !p.exists()) {
        let _ = write_config(handle, &cfg);
    }
    let gs = app.global_shortcut();
    for s in keys.all() {
        if let Err(e) = gs.register(s) {
            eprintln!("cannot register {s}: {e}");
        }
    }
    if let Err(e) = apply_autostart(handle, cfg.launch_at_login) {
        eprintln!("launch at login: {e}");
    }
    app.manage(Settings {
        cfg: Mutex::new(cfg),
        keys: Mutex::new(keys),
    });
}

#[cfg(test)]
mod tests {
    use super::{Config, Keys};

    #[test]
    fn keys_parse_and_dupes_rejected() {
        assert!(Keys::parse(&Config::default()).is_ok());
        let c = Config {
            full: "Cmd+Shift+2".into(),
            ..Config::default()
        };
        assert!(Keys::parse(&c).is_err());
        let c = Config {
            area: "nonsense++".into(),
            ..Config::default()
        };
        assert!(Keys::parse(&c).is_err());
    }
}
