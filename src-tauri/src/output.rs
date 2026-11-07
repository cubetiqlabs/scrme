use crate::config::*;
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
pub(crate) fn copy_image(app: &AppHandle, img: &RgbaImage) {
    #[cfg(target_os = "macos")]
    {
        let _ = app; // native pasteboard path needs no plugin
        if let Err(e) = crate::pasteboard::set_tiff(crate::tiff::rgb_tiff(img)) {
            eprintln!("clipboard write failed: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let (w, h) = img.dimensions();
        let image = Image::new_owned(img.as_raw().clone(), w, h);
        let _ = app.clipboard().write_image(&image);
    }
}

/// `~/Pictures/scrme`, created on demand.
fn pictures_dir() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let dir = std::path::Path::new(&home).join("Pictures/scrme");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Autosave without a second encode: the capture is already a PNG, so copy that file.
fn autosave_copy(src: &std::path::Path) -> Result<(), String> {
    let name = src.file_name().ok_or("no file name")?;
    std::fs::copy(src, pictures_dir()?.join(name))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(crate) fn save_png(img: &RgbaImage) -> Result<String, String> {
    let dir = pictures_dir()?;
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
    // Held until the capture's PNG exists, so the preview's buttons (Edit, Save as, drag-out)
    // never see the previous capture while this one is still being written.
    let latest = app.state::<crate::store::Latest>();
    let mut slot = latest.0.lock().unwrap();
    if cfg.animation {
        // Preview first: it only needs a small thumbnail, so it is on screen within a few ms,
        // while the (slower) clipboard write and PNG encode follow right after.
        crate::preview::show_preview(app, &img);
    }
    copy_image(app, &img);
    crate::store::put(app, &img, &mut slot);
    if cfg.autosave
        && let Some(path) = slot.as_ref()
        && let Err(e) = autosave_copy(path)
    {
        eprintln!("autosave failed: {e}");
    }
    drop(slot);
    drop(img); // nothing big stays in memory: the capture now lives on disk
    trim_memory();
}
