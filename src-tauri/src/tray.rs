use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager};

use crate::actions::{Action, dispatch};
use crate::config::{Config, Settings};
use crate::update::UpdateState;

/// Capture items whose native (gray) shortcut hint follows the config.
pub(crate) struct TrayKeys {
    pub(crate) area: MenuItem<tauri::Wry>,
    pub(crate) full: MenuItem<tauri::Wry>,
    pub(crate) window: MenuItem<tauri::Wry>,
}

pub(crate) fn sync_menu_keys(app: &AppHandle, cfg: &Config) {
    let Some(t) = app.try_state::<TrayKeys>() else {
        return;
    };
    for (item, key) in [
        (&t.area, &cfg.area),
        (&t.full, &cfg.full),
        (&t.window, &cfg.window),
    ] {
        let _ = item.set_accelerator(Some(key.as_str()));
    }
}

/// Menu-bar icon + menu. NSMenuItem images don't render in status-item menus on
/// recent macOS, so icons are Unicode glyph prefixes; shortcuts are native
/// accelerators (shown gray) kept in sync by `sync_menu_keys`.
pub(crate) fn build(app: &App) -> tauri::Result<()> {
    let cfg = app.state::<Settings>().cfg.lock().unwrap().clone();
    let item = |a: Action, label: &str, key: Option<&str>| {
        MenuItem::with_id(app, a.name(), label, true, key)
    };
    let area = item(Action::Area, "⛶  Capture Area", Some(&cfg.area))?;
    let full = item(Action::Screen, "▭  Capture Screen", Some(&cfg.full))?;
    let window = item(Action::Window, "❏  Capture Window", Some(&cfg.window))?;
    let upd = item(Action::Update, "↻  Check for Updates…", None)?;
    let prefs = item(Action::Prefs, "⚙︎  Preferences…", Some("Cmd+,"))?;
    let quit = item(Action::Quit, "⏻  Quit scrme", Some("Cmd+Q"))?;
    *app.state::<UpdateState>().item.lock().unwrap() = Some(upd.clone());
    app.manage(TrayKeys {
        area: area.clone(),
        full: full.clone(),
        window: window.clone(),
    });
    let menu = Menu::with_items(
        app,
        &[
            &area,
            &full,
            &window,
            &PredefinedMenuItem::separator(app)?,
            &upd,
            &prefs,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    TrayIconBuilder::new()
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, e| {
            if let Some(action) = Action::from_name(e.id.as_ref()) {
                dispatch(app, action);
            }
        })
        .build(app)?;
    Ok(())
}
