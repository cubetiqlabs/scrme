//! Where scrme keeps its files, without leaning on an engine's path API.
//!
//! The macOS locations match what Tauri used (`app_config_dir` / `app_cache_dir` for the
//! bundle identifier), so existing configs and captures are picked up unchanged.

use std::path::PathBuf;

/// Bundle identifier / application id.
pub const ID: &str = "com.sombochea.scrme";

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// `~/Library/Application Support/com.sombochea.scrme` on macOS.
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support").join(ID))
    } else if cfg!(target_os = "windows") {
        env_path("APPDATA").map(|d| d.join(ID))
    } else {
        env_path("XDG_CONFIG_HOME")
            .or_else(|| home().map(|h| h.join(".config")))
            .map(|d| d.join(ID))
    }
}

/// `~/Library/Caches/com.sombochea.scrme` on macOS.
pub fn cache_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches").join(ID))
    } else if cfg!(target_os = "windows") {
        env_path("LOCALAPPDATA").map(|d| d.join(ID))
    } else {
        env_path("XDG_CACHE_HOME")
            .or_else(|| home().map(|h| h.join(".cache")))
            .map(|d| d.join(ID))
    }
}

/// `~/Pictures/scrme` (the optional autosave folder).
pub fn pictures_dir() -> Option<PathBuf> {
    home().map(|h| h.join("Pictures").join("scrme"))
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.json"))
}

/// Folder holding the newest capture (`captures/` under the cache dir).
pub fn captures_dir() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("captures"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_absolute_and_under_the_app_id() {
        for dir in [config_dir(), cache_dir()] {
            let dir = dir.expect("HOME is set on CI");
            assert!(dir.is_absolute(), "{dir:?}");
            assert!(dir.ends_with(ID), "{dir:?}");
        }
        assert!(config_path().unwrap().ends_with("config.json"));
        assert!(captures_dir().unwrap().ends_with("captures"));
    }
}
