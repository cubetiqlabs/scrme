//! scrme: tiny, fast screenshot app. Module map:
//! `core` (`action`, `capture` freeze + crop, `store`, `output`, `config`, `host`) is
//! engine-agnostic · `actions` (hotkey/tray/CLI dispatch) · `toast` · `windows`
//! (overlay/settings/editor) · `commands` (webview IPC) · `tray` · `update` are the Tauri engine.
//!
//! The GPUI engine (issue #63) lives in `gpui_app` and reuses the same core through
//! [`host::Host`].

mod action;
mod capture;
mod config;
mod host;
mod output;
#[cfg(target_os = "macos")]
mod pasteboard;
mod paths;
mod platform;
mod preview;
mod state;
mod store;
mod tiff;
mod toast;

#[cfg(feature = "tauri-app")]
mod actions;
#[cfg(feature = "tauri-app")]
mod commands;
#[cfg(feature = "tauri-app")]
mod tauri_host;
#[cfg(feature = "tauri-app")]
mod tray;
#[cfg(feature = "tauri-app")]
mod update;
#[cfg(feature = "tauri-app")]
mod windows;

#[cfg(feature = "gpui-app")]
pub mod gpui_app;

#[cfg(feature = "tauri-app")]
use std::sync::Arc;

#[cfg(feature = "tauri-app")]
use tauri::Manager;
#[cfg(feature = "tauri-app")]
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(feature = "tauri-app")]
use action::Action;
#[cfg(feature = "tauri-app")]
use actions::{dispatch, handle_args};

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First: a second launch (or `scrme --screen`) is forwarded to the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_args(app, argv.iter().skip(1));
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, sc, ev| {
                    if ev.state != ShortcutState::Pressed {
                        return;
                    }
                    let keys = *app.state::<config::Hotkeys>().0.lock().unwrap();
                    if let Some(action) = Action::from_shortcut(&keys, sc) {
                        dispatch(app, action);
                    }
                })
                .build(),
        )
        .manage(update::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            commands::commit,
            commands::cancel,
            commands::edit_size,
            commands::edit_bytes,
            commands::copy_rgba,
            commands::save_rgba,
            commands::copy_text,
            commands::pin_rgba,
            commands::commit_editor,
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

            // The core reads the config, the newest capture and the host without an AppHandle.
            if let Some(dir) = paths::captures_dir() {
                store::init(dir);
            }
            host::install(Arc::new(tauri_host::TauriHost::new(app.handle().clone())));
            config::init_tauri(app);
            tray::build(app)?;

            // Non-macOS: warm the hidden webview overlay/toast in the background so startup
            // returns immediately. macOS draws both natively (no webview, no helper processes).
            #[cfg(not(target_os = "macos"))]
            {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    let _ = windows::build_overlay(&h);
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    let _ = toast::build_webview(&h, None);
                });
            }
            update::spawn_auto_check(app.handle());

            // Flags given on a cold start (`scrme --prefs`) behave like a forwarded launch.
            handle_args(app.handle(), std::env::args().skip(1));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // scrme lives in the menu bar; destroying a webview window (the parked editor after
            // EDITOR_KEEP_SECS, or Preferences) must not quit the app. Tauri requests an exit
            // with `code: None` when the last window is destroyed. Explicit quits and updater
            // restarts arrive with `code: Some(_)`, so let those through.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
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
