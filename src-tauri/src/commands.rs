use image::RgbaImage;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::capture;
use crate::config::{self, Config, Hotkeys, Keys, apply_autostart};
use crate::host;
use crate::output::trim_memory;
use crate::paths;
use crate::preview;
use crate::state;
use crate::store;
use crate::tray::sync_menu_keys;
use crate::windows;

#[tauri::command]
pub(crate) fn commit(app: AppHandle, x: i64, y: i64, w: i64, h: i64, edit: bool) {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
    capture::commit_selection(x, y, w, h, edit);
}

#[tauri::command]
pub(crate) fn cancel(app: AppHandle) {
    state::clear_frozen();
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
    trim_memory();
}

#[tauri::command]
pub(crate) fn edit_size() -> Option<(u32, u32)> {
    state::pending_size()
}

#[tauri::command]
pub(crate) fn edit_bytes() -> Response {
    // The editor loads once: hand the buffer over instead of cloning it (halves peak memory).
    Response::new(
        state::take_pending()
            .map(|i| i.into_raw())
            .unwrap_or_default(),
    )
}

/// Body = raw RGBA, headers `w`/`h` = dimensions.
pub(crate) fn rgba_from(req: &tauri::ipc::Request) -> Option<RgbaImage> {
    let tauri::ipc::InvokeBody::Raw(raw) = req.body() else {
        return None;
    };
    let num = |k| req.headers().get(k)?.to_str().ok()?.parse::<u32>().ok();
    RgbaImage::from_raw(num("w")?, num("h")?, raw.clone())
}

#[tauri::command]
pub(crate) fn copy_rgba(req: tauri::ipc::Request) -> Result<(), String> {
    let img = rgba_from(&req).ok_or("bad image")?;
    host::host().copy_image(&img);
    store::replace_latest(&img);
    trim_memory();
    Ok(())
}

/// Float the edited screenshot above everything (native window; macOS only for now).
#[tauri::command]
pub(crate) fn pin_rgba(req: tauri::ipc::Request) -> Result<(), String> {
    let img = rgba_from(&req).ok_or("bad image")?;
    store::replace_latest(&img);
    preview::show_pin(&host::host(), img)
}

/// Plain text to the clipboard (e.g. the eyedropper's hex).
#[tauri::command]
pub(crate) fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn save_rgba(req: tauri::ipc::Request) -> Result<String, String> {
    let img = rgba_from(&req).ok_or("bad image")?;
    let path = crate::output::save_png(&img)?;
    store::replace_latest(&img);
    Ok(path)
}

/// Persist the editor's current canvas as the latest capture (called when closing it).
#[tauri::command]
pub(crate) fn commit_editor(req: tauri::ipc::Request) -> Result<(), String> {
    let img = rgba_from(&req).ok_or("bad image")?;
    store::replace_latest(&img);
    trim_memory();
    Ok(())
}

#[tauri::command]
pub(crate) fn close_editor(app: AppHandle) {
    state::take_pending();
    // Hide + release the image, keep the webview warm briefly (instant re-open), then destroy it.
    windows::park_editor(&app);
    trim_memory();
}

#[derive(Serialize)]
pub(crate) struct About {
    pub(crate) version: String,
    pub(crate) commit: &'static str,
    pub(crate) built: &'static str,
    pub(crate) license: &'static str,
    pub(crate) author: &'static str,
    pub(crate) repo: &'static str,
}

#[tauri::command]
pub(crate) fn about_info(app: AppHandle) -> About {
    About {
        version: app.package_info().version.to_string(),
        commit: env!("SCRME_COMMIT"),
        built: env!("SCRME_BUILT"),
        license: "MIT",
        author: "Sambo Chea",
        repo: REPO_URL,
    }
}

pub(crate) const REPO_URL: &str = "https://github.com/cubetiqlabs/scrme";

/// Open a GitHub link in the default browser (allow-listed; the webview is not trusted with arbitrary URLs).
#[tauri::command]
pub(crate) fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://github.com/") && url.chars().all(|c| c.is_ascii_graphic())) {
        return Err("link not allowed".into());
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn close_window(window: tauri::WebviewWindow) {
    let _ = window.close();
}

#[tauri::command]
pub(crate) fn default_config() -> Config {
    Config::default()
}

#[tauri::command]
pub(crate) fn get_config() -> Config {
    config::current()
}

/// Validate, re-register hotkeys live (rolling back on failure), persist.
#[tauri::command]
pub(crate) fn set_config(app: AppHandle, cfg: Config) -> Result<(), String> {
    let new = Keys::parse(&cfg)?;
    let state = app.state::<Hotkeys>();
    let old = *state.0.lock().unwrap();
    let was_login = config::current().launch_at_login;
    apply_autostart(&app, cfg.launch_at_login)?;
    let gs = app.global_shortcut();
    let register = |k: Keys| -> Result<(), String> {
        gs.unregister_all().map_err(|e| e.to_string())?;
        for s in k.all() {
            gs.register(s).map_err(|e| format!("{s}: {e}"))?;
        }
        Ok(())
    };
    if let Err(e) = register(new) {
        let _ = register(old);
        let _ = apply_autostart(&app, was_login);
        return Err(e);
    }
    *state.0.lock().unwrap() = new;
    if let Some(path) = paths::config_path() {
        let _ = config::save(&path, &cfg);
    }
    sync_menu_keys(&app, &cfg);
    config::set(cfg);
    Ok(())
}
