//! scrme: tiny, fast screenshot app. Module map:
//! `actions` (hotkey/tray/CLI dispatch) · `capture` (freeze + crop) · `output` (clipboard/save) ·
//! `toast` · `windows` (overlay/settings/editor) · `commands` (webview IPC) · `config` · `state` ·
//! `tray` · `update`.

mod actions;
mod capture;
mod commands;
mod config;
mod output;
#[cfg(target_os = "macos")]
mod pasteboard;
mod state;
mod tiff;
mod toast;
mod tray;
mod update;
mod windows;

use std::time::Duration;

use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;

use actions::{Action, dispatch, handle_args};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First: a second launch (or `scrme --screen`) is forwarded to the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_args(app, argv.iter().skip(1));
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, sc, ev| {
                    if ev.state != ShortcutState::Pressed {
                        return;
                    }
                    let keys = *app.state::<config::Settings>().keys.lock().unwrap();
                    if let Some(action) = Action::from_shortcut(&keys, sc) {
                        dispatch(app, action);
                    }
                })
                .build(),
        )
        .manage(state::Frozen::default())
        .manage(state::Pending::default())
        .manage(update::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            commands::commit,
            commands::cancel,
            commands::edit_size,
            commands::edit_bytes,
            commands::copy_rgba,
            commands::save_rgba,
            commands::close_editor,
            commands::get_config,
            commands::set_config,
            commands::default_config,
            commands::about_info,
            commands::open_url,
            commands::close_window,
            update::check_update,
            update::update_ready,
            update::restart_app
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            config::init(app);
            tray::build(app)?;

            // Tray + hotkeys are live; warm the hidden windows in the background so
            // startup returns immediately. (A hotkey before this finishes builds the overlay itself.)
            let h = app.handle().clone();
            std::thread::spawn(move || {
                let _ = windows::build_overlay(&h);
                std::thread::sleep(Duration::from_millis(800));
                let _ = toast::build_toast(&h, None);
            });
            update::spawn_auto_check(app.handle());

            // Flags given on a cold start (`scrme --prefs`) behave like a forwarded launch.
            handle_args(app.handle(), std::env::args().skip(1));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_info_is_iso8601_utc() {
        let b = env!("SCRME_BUILT");
        let ok = b.len() == 20
            && b.ends_with('Z')
            && b.as_bytes()[10] == b'T'
            && b[..4].parse::<u32>().is_ok_and(|y| y >= 2026);
        assert!(ok, "bad build time: {b}");
        println!("built {b} commit {}", env!("SCRME_COMMIT"));
    }
}
