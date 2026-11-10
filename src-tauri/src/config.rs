use std::path::Path;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

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

/// The live config, read by the capture path on every capture. One lock-free read for
/// readers, and writes only happen from Preferences.
static CFG: OnceLock<RwLock<Config>> = OnceLock::new();

pub(crate) fn init(cfg: Config) {
    let _ = CFG.set(RwLock::new(cfg));
}

pub(crate) fn current() -> Config {
    CFG.get()
        .map(|c| c.read().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default()
}

pub(crate) fn set(cfg: Config) {
    if let Some(lock) = CFG.get() {
        *lock.write().unwrap_or_else(|e| e.into_inner()) = cfg;
    }
}

pub(crate) fn load(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub(crate) fn save(path: &Path, cfg: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg).unwrap())
}

/// Load (or create) the config file and publish it to the capture path.
pub(crate) fn init_from_disk() -> Config {
    let Some(path) = crate::paths::config_path() else {
        let cfg = Config::default();
        init(cfg.clone());
        return cfg;
    };
    let cfg = load(&path);
    if !path.exists() {
        let _ = save(&path, &cfg);
    }
    init(cfg.clone());
    cfg
}

#[cfg(feature = "tauri-app")]
mod tauri_side {
    use std::sync::Mutex;

    use tauri::{AppHandle, Manager};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    use super::Config;

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

    /// Tauri-managed state: the parsed (registered) shortcuts.
    pub(crate) struct Hotkeys(pub(crate) Mutex<Keys>);

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

    /// Load config, register the global hotkeys and publish the parsed keys as app state.
    pub(crate) fn init(app: &tauri::App) {
        let handle = app.handle();
        let cfg = super::init_from_disk();
        let keys = Keys::parse(&cfg).unwrap_or_else(|_| Keys::parse(&Config::default()).unwrap());
        let gs = app.global_shortcut();
        for s in keys.all() {
            if let Err(e) = gs.register(s) {
                eprintln!("cannot register {s}: {e}");
            }
        }
        if let Err(e) = apply_autostart(handle, cfg.launch_at_login) {
            eprintln!("launch at login: {e}");
        }
        app.manage(Hotkeys(Mutex::new(keys)));
    }
}

#[cfg(feature = "tauri-app")]
pub(crate) use tauri_side::{Hotkeys, Keys, apply_autostart, init as init_tauri};

#[cfg(all(test, feature = "tauri-app"))]
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

#[cfg(test)]
mod config_tests {
    use super::{Config, load, save};

    #[test]
    fn round_trips_and_unknown_fields_are_ignored() {
        let dir = std::env::temp_dir().join(format!("scrme-cfg-{}", std::process::id()));
        let path = dir.join("config.json");
        let cfg = Config {
            area: "Ctrl+Alt+A".into(),
            autosave: true,
            ..Config::default()
        };
        save(&path, &cfg).unwrap();
        let back = load(&path);
        assert_eq!(back.area, "Ctrl+Alt+A");
        assert!(back.autosave);
        // Older files with extra keys still load (serde ignores unknown fields by default).
        std::fs::write(&path, r#"{"area":"Ctrl+Alt+B","gone":1}"#).unwrap();
        assert_eq!(load(&path).area, "Ctrl+Alt+B");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
