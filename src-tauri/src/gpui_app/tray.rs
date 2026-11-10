//! The menu-bar icon and its menu, with `tray-icon` + `muda` (what Tauri wraps).
//! Icons are Unicode glyph prefixes in the titles, as in the Tauri build: NSMenuItem images
//! do not render in status-item menus on recent macOS.

use std::cell::RefCell;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::action::Action;
use crate::config::Config;

struct Tray {
    /// Dropping the icon removes it from the menu bar, so it lives for the whole process.
    _icon: TrayIcon,
    area: MenuItem,
    full: MenuItem,
    window: MenuItem,
}

thread_local! {
    /// AppKit objects: main thread only, and not `Send`, so no `static` (which requires `Sync`).
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Build the status item + menu. Must run on the main thread, once.
pub(crate) fn build(cfg: &Config, tx: async_channel::Sender<Action>) -> Result<(), String> {
    let item = |action: Action, label: &str, key: Option<&str>| {
        MenuItem::with_id(action.name(), label, true, key.and_then(accelerator))
    };
    let area = item(Action::Area, "⛶  Capture Area", Some(&cfg.area));
    let full = item(Action::Screen, "▭  Capture Screen", Some(&cfg.full));
    let window = item(Action::Window, "❏  Capture Window", Some(&cfg.window));
    let last = item(Action::Last, "⟲  Last Capture", None);
    let prefs = item(Action::Prefs, "⚙︎  Preferences…", Some("Cmd+,"));
    let quit = item(Action::Quit, "⏻  Quit scrme", Some("Cmd+Q"));
    let menu = Menu::new();
    menu.append_items(&[
        &area,
        &full,
        &window,
        &last,
        &PredefinedMenuItem::separator(),
        &prefs,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .map_err(|e| e.to_string())?;

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(action) = Action::from_name(event.id.as_ref()) {
            let _ = tx.send_blocking(action);
        }
    }));

    let (rgba, width, height) = tray_rgba()?;
    let icon = Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_icon_templated(icon)
        .with_menu(Box::new(menu))
        .with_tooltip("scrme")
        .build()
        .map_err(|e| e.to_string())?;
    TRAY.with(|slot| {
        *slot.borrow_mut() = Some(Tray {
            _icon: tray,
            area,
            full,
            window,
        });
    });
    Ok(())
}

/// Keep the gray shortcut hints in the menu in step with the config.
pub(crate) fn sync_keys(cfg: &Config) {
    TRAY.with(|slot| {
        let slot = slot.borrow();
        let Some(tray) = slot.as_ref() else {
            return;
        };
        for (item, key) in [
            (&tray.area, &cfg.area),
            (&tray.full, &cfg.full),
            (&tray.window, &cfg.window),
        ] {
            let _ = item.set_accelerator(accelerator(key));
        }
    });
}

/// `Cmd+Shift+2` → the native menu accelerator hint (same parser as the hotkeys).
fn accelerator(key: &str) -> Option<tray_icon::menu::accelerator::Accelerator> {
    key.parse().ok()
}

/// The template menu-bar icon, decoded from the same PNG the Tauri build uses.
fn tray_rgba() -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory(include_bytes!("../../icons/tray.png"))
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}
