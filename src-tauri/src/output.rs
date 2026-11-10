use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::{config, host, paths, preview, store};

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

/// Put an image on the system clipboard (native on macOS, no plugin, no retained buffers).
pub(crate) fn copy_image(img: &RgbaImage) {
    #[cfg(target_os = "macos")]
    if let Err(e) = crate::pasteboard::set_tiff(crate::tiff::rgb_tiff(img)) {
        eprintln!("clipboard write failed: {e}");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = img;
}

/// `~/Pictures/scrme`, created on demand.
fn pictures_dir() -> Result<PathBuf, String> {
    let dir = paths::pictures_dir().ok_or("no home directory")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Autosave without a second encode: the capture is already a PNG, so copy that file.
fn autosave_copy(src: &Path) -> Result<(), String> {
    let name = src.file_name().ok_or("no file name")?;
    std::fs::copy(src, pictures_dir()?.join(name))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Used by the Tauri editor's Save button; the GPUI editor (phase 3) will use it too.
#[cfg_attr(not(feature = "tauri-app"), allow(dead_code))]
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
pub(crate) fn finish(img: RgbaImage) {
    let cfg = config::current();
    let host = host::host();
    // Held until the capture's PNG exists, so the preview's buttons (Edit, Save as, drag-out)
    // never see the previous capture while this one is still being written.
    let mut latest = store::lock();
    if cfg.animation {
        // Preview first: it only needs a small thumbnail, so it is on screen within a few ms,
        // while the (slower) clipboard write and PNG encode follow right after.
        preview::show_preview(&host, &img);
    }
    host.copy_image(&img);
    if let Some(slot) = latest.as_mut() {
        slot.put(&img);
    }
    if cfg.autosave
        && let Some(path) = latest.as_ref().and_then(|slot| slot.path())
        && let Err(e) = autosave_copy(path)
    {
        eprintln!("autosave failed: {e}");
    }
    drop(latest);
    drop(img); // nothing big stays in memory: the capture now lives on disk
    trim_memory();
}
