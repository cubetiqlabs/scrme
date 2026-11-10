//! Tauri's side of the action table: hotkey → action matching and dispatch.
//! The actions themselves are named in [`crate::action`], shared with the GPUI engine.

use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::Shortcut;

use crate::action::Action;
use crate::capture::{capture_full, capture_window, start_area_capture};
use crate::config::Keys;
use crate::host;
use crate::preview;
use crate::store;
use crate::toast;
use crate::update::{UpdateState, run_update};
use crate::windows::open_settings;

impl Action {
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
                Action::Area => start_area_capture(),
                Action::Screen => capture_full(),
                _ => capture_window(),
            });
        }
        // Let a closing tray menu finish first; it steals activation on its way out.
        Action::Prefs => {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                open_settings(&app);
            });
        }
        // Reopen the newest capture (kept on disk) in the editor.
        Action::Last => {
            std::thread::spawn(
                move || match store::latest_path().and_then(|p| store::load(&p)) {
                    Some(img) => preview::open_editor_with(img),
                    None => toast::err(&host::host(), "No capture yet"),
                },
            );
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
    for action in Action::from_args(args) {
        dispatch(app, action);
    }
}
