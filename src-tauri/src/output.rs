use crate::{config::*, toast::*};
use image::RgbaImage;
use tauri::image::Image;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

pub(crate) fn copy_image(app: &AppHandle, img: RgbaImage) {
    let (w, h) = img.dimensions();
    let _ = app
        .clipboard()
        .write_image(&Image::new_owned(img.into_raw(), w, h));
}

pub(crate) fn save_png(img: &RgbaImage) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let dir = std::path::Path::new(&home).join("Pictures/scrme");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = dir.join(format!("scrme-{ms}.png"));
    img.save(&path).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Quick-capture finish: clipboard first (that's the "instant" part),
/// then the toast, then the optional file write.
pub(crate) fn finish(app: &AppHandle, img: RgbaImage) {
    let cfg = app.state::<Settings>().cfg.lock().unwrap().clone();
    let saved = cfg.autosave.then(|| img.clone());
    copy_image(app, img);
    if cfg.animation {
        show_toast(
            app,
            if cfg.autosave {
                "Copied & saved"
            } else {
                "Copied"
            },
        );
    }
    if let Some(img) = saved
        && let Err(e) = save_png(&img)
    {
        eprintln!("autosave failed: {e}");
    }
}
