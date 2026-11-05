use std::time::Instant;

use crate::{capture::*, config::*, output::*, state::*, tray::*, windows::*};
use image::RgbaImage;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[tauri::command]
pub(crate) fn commit(
    app: AppHandle,
    state: tauri::State<Frozen>,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    edit: bool,
) {
    let t = Instant::now();
    // "Capture & annotate" opens the editor regardless of ⇧; always clear the flag.
    let edit = app
        .state::<EditMode>()
        .0
        .swap(false, std::sync::atomic::Ordering::SeqCst)
        || edit;
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
    let Some(frame) = state.0.lock().unwrap().take() else {
        return;
    };
    let (iw, ih) = frame.size();
    let Some((cx, cy, cw, ch)) = clamp_rect(x, y, w, h, iw, ih) else {
        return;
    };
    let Some(crop) = frame.crop(cx, cy, cw, ch) else {
        return;
    };
    drop(frame); // release the screenshot before the (slower) clipboard/editor work
    if edit {
        *app.state::<Pending>().0.lock().unwrap() = Some(crop);
        open_editor(&app, cw, ch);
    } else {
        finish(&app, crop);
    }
    eprintln!("commit {:?}", t.elapsed());
}

#[tauri::command]
pub(crate) fn cancel(app: AppHandle, state: tauri::State<Frozen>) {
    app.state::<EditMode>()
        .0
        .store(false, std::sync::atomic::Ordering::SeqCst);
    state.0.lock().unwrap().take();
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
    trim_memory();
}

#[tauri::command]
pub(crate) fn edit_size(state: tauri::State<Pending>) -> Option<(u32, u32)> {
    state
        .0
        .lock()
        .unwrap()
        .as_ref()
        .map(|i| (i.width(), i.height()))
}

#[tauri::command]
pub(crate) fn edit_bytes(state: tauri::State<Pending>) -> Response {
    // The editor loads once: hand the buffer over instead of cloning it (halves peak memory).
    Response::new(
        state
            .0
            .lock()
            .unwrap()
            .take()
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
pub(crate) fn copy_rgba(app: AppHandle, req: tauri::ipc::Request) -> Result<(), String> {
    let img = rgba_from(&req).ok_or("bad image")?;
    copy_image(&app, img);
    trim_memory();
    Ok(())
}

/// Plain text to the clipboard (e.g. the eyedropper's hex).
#[tauri::command]
pub(crate) fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn save_rgba(req: tauri::ipc::Request) -> Result<String, String> {
    save_png(&rgba_from(&req).ok_or("bad image")?)
}

#[tauri::command]
pub(crate) fn close_editor(app: AppHandle, state: tauri::State<Pending>) {
    state.0.lock().unwrap().take();
    // Destroy (not hide): frees the webview process and the image; it is rebuilt on the next edit.
    if let Some(w) = app.get_webview_window("editor") {
        let _ = w.destroy();
    }
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
pub(crate) fn get_config(state: tauri::State<Settings>) -> Config {
    state.cfg.lock().unwrap().clone()
}

/// Validate, re-register hotkeys live (rolling back on failure), persist.
#[tauri::command]
pub(crate) fn set_config(
    app: AppHandle,
    state: tauri::State<Settings>,
    cfg: Config,
) -> Result<(), String> {
    let new = Keys::parse(&cfg)?;
    let old = *state.keys.lock().unwrap();
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
        return Err(e);
    }
    *state.keys.lock().unwrap() = new;
    let _ = write_config(&app, &cfg);
    sync_menu_keys(&app, &cfg);
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}
