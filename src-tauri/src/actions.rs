//! Every user-triggerable action funnels through here: global hotkeys, the tray
//! menu and CLI flags (`scrme --screen`, handy for scripts and measurements).

use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::Shortcut;

use crate::capture::{capture_full, capture_window, start_area_capture};
use crate::config::Keys;
use crate::update::{UpdateState, run_update};
use crate::windows::open_settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Area,
    Screen,
    Window,
    Prefs,
    Update,
    Quit,
}

/// One table = tray item id = CLI flag (`--<name>`).
const NAMES: [(Action, &str); 6] = [
    (Action::Area, "area"),
    (Action::Screen, "screen"),
    (Action::Window, "window"),
    (Action::Prefs, "prefs"),
    (Action::Update, "update"),
    (Action::Quit, "quit"),
];

impl Action {
    pub(crate) fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(a, _)| *a == self)
            .map_or("", |(_, n)| n)
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        NAMES.iter().find(|(_, n)| *n == name).map(|(a, _)| *a)
    }

    pub(crate) fn from_flag(flag: &str) -> Option<Self> {
        Self::from_name(flag.strip_prefix("--")?)
    }

    pub(crate) fn from_shortcut(keys: &Keys, sc: &Shortcut) -> Option<Self> {
        [
            (Action::Area, keys.area),
            (Action::Screen, keys.full),
            (Action::Window, keys.window),
        ]
        .into_iter()
        .find(|(_, k)| k == sc)
        .map(|(a, _)| a)
    }
}

/// Run an action. Heavy work goes off the main thread so the event loop never stalls.
pub(crate) fn dispatch(app: &AppHandle, action: Action) {
    let app = app.clone();
    match action {
        Action::Area | Action::Screen | Action::Window => {
            std::thread::spawn(move || match action {
                Action::Area => start_area_capture(&app),
                Action::Screen => capture_full(&app),
                _ => capture_window(&app),
            });
        }
        // Let a closing tray menu finish first; it steals activation on its way out.
        Action::Prefs => {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                open_settings(&app);
            });
        }
        Action::Update => {
            if app.state::<UpdateState>().ready.lock().unwrap().is_some() {
                app.restart();
            }
            tauri::async_runtime::spawn(async move {
                let _ = run_update(&app, true).await;
            });
        }
        Action::Quit => app.exit(0),
    }
}

/// Dispatch every recognised `--flag` in `args`.
pub(crate) fn handle_args<I, S>(app: &AppHandle, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for a in args {
        if let Some(action) = Action::from_flag(a.as_ref()) {
            dispatch(app, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, NAMES};

    #[test]
    fn names_round_trip() {
        for (a, n) in NAMES {
            assert_eq!(a.name(), n);
            assert_eq!(Action::from_name(n), Some(a));
            assert_eq!(Action::from_flag(&format!("--{n}")), Some(a));
        }
    }

    #[test]
    fn unknown_flags_are_ignored() {
        assert_eq!(Action::from_flag("--nope"), None);
        assert_eq!(Action::from_flag("area"), None); // flags need the dashes
        assert_eq!(Action::from_name(""), None);
    }
}
