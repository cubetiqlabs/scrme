use crate::{config::*, toast::*};
use image::RgbaImage;
#[cfg(not(target_os = "macos"))]
use tauri::image::Image;
use tauri::{AppHandle, Manager};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Hand freed-but-cached pages back to the OS. libmalloc keeps freed large blocks (our
/// ~30 MB screenshot buffers) cached, which Activity Monitor shows as 100+ MB of growth.
pub(crate) fn trim_memory() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: documented libmalloc call; a null zone means "all zones", goal 0 = as much as possible.
        unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
    }
}

/// Put an image on the system clipboard.
pub(crate) fn copy_image(app: &AppHandle, img: RgbaImage) {
    #[cfg(target_os = "macos")]
    {
        let _ = app; // native pasteboard path needs no plugin
        if let Err(e) = crate::pasteboard::set_tiff(crate::tiff::rgb_tiff(&img)) {
            eprintln!("clipboard write failed: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let (w, h) = img.dimensions();
        let image = Image::new_owned(img.into_raw(), w, h);
        let _ = app.clipboard().write_image(&image);
    }
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
    trim_memory();
}
