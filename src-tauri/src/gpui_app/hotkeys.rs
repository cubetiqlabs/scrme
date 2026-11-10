//! Global hotkeys through `global-hotkey` (the crate Tauri's plugin wraps, so the config
//! strings are the same `Cmd+Shift+2` style). The manager lives on the main thread and every
//! registration is re-applied when Preferences changes a shortcut.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::action::Action;
use crate::config::Config;

/// The three capture shortcuts plus the action each triggers.
type Bound = [(HotKey, Action); 3];

struct Registry {
    manager: GlobalHotKeyManager,
    /// The hotkeys currently registered with the OS.
    registered: Mutex<Vec<HotKey>>,
    /// Event id → action (the OS reports events by id, not by key).
    bound: Mutex<HashMap<u32, Action>>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn parse(value: &str, name: &str) -> Result<HotKey, String> {
    value.parse::<HotKey>().map_err(|e| format!("{name}: {e}"))
}

/// The shortcuts named in the config, rejecting duplicates.
pub(crate) fn parse_all(cfg: &Config) -> Result<Bound, String> {
    let keys: Bound = [
        (parse(&cfg.area, "Area")?, Action::Area),
        (parse(&cfg.full, "Screen")?, Action::Screen),
        (parse(&cfg.window, "Window")?, Action::Window),
    ];
    let dupes = keys
        .iter()
        .enumerate()
        .any(|(i, (k, _))| keys[..i].iter().any(|(other, _)| other == k));
    if dupes {
        return Err("Each action needs a different shortcut".into());
    }
    Ok(keys)
}

/// Register the configured shortcuts and route events into the UI channel.
/// Must run on the main thread, once.
pub(crate) fn install(cfg: &Config, tx: async_channel::Sender<Action>) -> Result<(), String> {
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    let _ = REGISTRY.set(Registry {
        manager,
        registered: Mutex::new(Vec::new()),
        bound: Mutex::new(HashMap::new()),
    });
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state != HotKeyState::Pressed {
            return;
        }
        if let Some(action) = action_for(event.id()) {
            // Unbounded: this never blocks the OS event thread.
            let _ = tx.send_blocking(action);
        }
    }));
    apply(cfg)
}

/// Replace the registered shortcuts with the ones in `cfg`. On success the config is live.
pub(crate) fn apply(cfg: &Config) -> Result<(), String> {
    let reg = REGISTRY.get().ok_or("hotkeys are not installed yet")?;
    let next = parse_all(cfg)?;
    let mut registered = reg.registered.lock().unwrap_or_else(|e| e.into_inner());
    for key in registered.drain(..) {
        let _ = reg.manager.unregister(key);
    }
    let mut bound = reg.bound.lock().unwrap_or_else(|e| e.into_inner());
    bound.clear();
    for (key, action) in next {
        reg.manager
            .register(key)
            .map_err(|e| format!("{}: {e}", action.name()))?;
        bound.insert(key.id(), action);
        registered.push(key);
    }
    Ok(())
}

fn action_for(id: u32) -> Option<Action> {
    let reg = REGISTRY.get()?;
    let bound = reg.bound.lock().unwrap_or_else(|e| e.into_inner());
    bound.get(&id).copied()
}

/// Turn a key press from Preferences into a config string both engines can parse.
pub(crate) fn from_keystroke(stroke: &gpui::Keystroke) -> Result<String, String> {
    let m = stroke.modifiers;
    // Shift alone would collide with typing, so require a real modifier.
    if !(m.platform || m.control || m.alt) {
        return Err("Hold ⌘, ⌃ or ⌥ together with a key".into());
    }
    let mut parts: Vec<String> = Vec::with_capacity(4);
    for (pressed, name) in [
        (m.platform, "Cmd"),
        (m.control, "Ctrl"),
        (m.alt, "Alt"),
        (m.shift, "Shift"),
    ] {
        if pressed {
            parts.push(name.to_string());
        }
    }
    parts.push(key_name(&stroke.key)?);
    let text = parts.join("+");
    // Reject anything the OS registration would refuse, before it reaches the config.
    parse(&text, "Shortcut").map(|_| text)
}

fn key_name(key: &str) -> Result<String, String> {
    let named = match key {
        "space" => "Space",
        "enter" | "return" => "Enter",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "," => ",",
        "." => ".",
        "/" => "/",
        ";" => ";",
        "'" => "'",
        "[" => "[",
        "]" => "]",
        "\\" => "\\",
        "-" => "-",
        "=" => "=",
        "`" => "`",
        single if single.len() == 1 && single.as_bytes()[0].is_ascii_alphabetic() => {
            return Ok(single.to_ascii_uppercase());
        }
        digit if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
            return Ok(digit.to_string());
        }
        fkey if fkey.starts_with('f') && fkey[1..].parse::<u8>().is_ok() => {
            return Ok(fkey.to_ascii_uppercase());
        }
        other => return Err(format!("{other} can't be part of a global shortcut")),
    };
    Ok(named.to_string())
}

#[cfg(test)]
mod tests {
    use super::{from_keystroke, parse_all};
    use crate::config::Config;
    use gpui::{Keystroke, Modifiers};

    fn stroke(mods: Modifiers, key: &str) -> Keystroke {
        Keystroke {
            modifiers: mods,
            key: key.into(),
            key_char: None,
        }
    }

    #[test]
    fn config_shortcuts_parse_and_duplicates_are_rejected() {
        assert!(parse_all(&Config::default()).is_ok());
        let cfg = Config {
            full: "Cmd+Shift+2".into(),
            ..Config::default()
        };
        assert!(parse_all(&cfg).is_err());
        let cfg = Config {
            area: "nonsense++".into(),
            ..Config::default()
        };
        assert!(parse_all(&cfg).is_err());
    }

    #[test]
    fn recorded_keys_become_config_strings() {
        let cmd_shift = Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            from_keystroke(&stroke(cmd_shift, "2")).unwrap(),
            "Cmd+Shift+2"
        );
        assert_eq!(
            from_keystroke(&stroke(cmd_shift, "a")).unwrap(),
            "Cmd+Shift+A"
        );
        assert_eq!(
            from_keystroke(&stroke(cmd_shift, "space")).unwrap(),
            "Cmd+Shift+Space"
        );
        assert_eq!(
            from_keystroke(&stroke(
                Modifiers {
                    platform: true,
                    ..Default::default()
                },
                "f5"
            ))
            .unwrap(),
            "Cmd+F5"
        );
        // A bare key (and shift alone) is not a usable global shortcut.
        assert!(from_keystroke(&stroke(Modifiers::none(), "a")).is_err());
        assert!(
            from_keystroke(&stroke(
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                "a"
            ))
            .is_err()
        );
    }
}
