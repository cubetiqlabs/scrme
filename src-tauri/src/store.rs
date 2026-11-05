//! The latest capture lives on disk, not in memory: a PNG in the app cache dir
//! (`~/Library/Caches/<id>/captures/`). The preview's Edit / Copy / Save as and drag-out all
//! read it from there, and "Last Capture" in the tray reopens it later, even after the
//! preview closed and the clipboard was overwritten. Only the newest file is kept.

use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::RgbaImage;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use tauri::{AppHandle, Manager};

/// Path of the newest capture. The mutex is held while a new one is being written, so
/// anything asking for "the latest" waits until its file really exists.
#[derive(Default)]
pub(crate) struct Latest(pub(crate) Mutex<Option<PathBuf>>);

fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join("captures"))
}

/// `2026-11-05 at 15.04.22` in local time (macOS screenshot style), without a date crate.
fn local_stamp() -> String {
    // SAFETY: `localtime_r` fills the zeroed `tm` we pass; `time` has no preconditions.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02} at {:02}.{:02}.{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// Write `img` as the new latest capture (fast PNG encode), replacing `previous`.
pub(crate) fn write_png(
    app: &AppHandle,
    img: &RgbaImage,
    previous: Option<PathBuf>,
) -> Option<PathBuf> {
    let dir = dir(app)?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("scrme {}.png", local_stamp()));
    encode(img, &path, &dir.join(".writing.png"))?;
    if let Some(old) = previous.filter(|p| *p != path) {
        let _ = std::fs::remove_file(old);
    }
    Some(path)
}

/// Fast PNG encode into `tmp`, then an atomic rename to `path`.
fn encode(img: &RgbaImage, path: &Path, tmp: &Path) -> Option<()> {
    let file = std::fs::File::create(tmp).ok()?;
    let enc =
        PngEncoder::new_with_quality(BufWriter::new(file), CompressionType::Fast, FilterType::Sub);
    img.write_with_encoder(enc).ok()?;
    std::fs::rename(tmp, path).ok()
}

/// A scratch PNG for an in-memory image (a pin being dragged out), overwritten each time.
pub(crate) fn write_scratch(app: &AppHandle, img: &RgbaImage) -> Option<PathBuf> {
    let dir = dir(app)?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("pin {}.png", local_stamp()));
    encode(img, &path, &dir.join(".scratch.png"))?;
    Some(path)
}

/// The newest capture's file, once any write in progress has finished.
pub(crate) fn latest_path(app: &AppHandle) -> Option<PathBuf> {
    app.state::<Latest>()
        .0
        .lock()
        .unwrap()
        .clone()
        .filter(|p| p.exists())
}

pub(crate) fn load(path: &Path) -> Option<RgbaImage> {
    image::open(path).ok().map(|i| i.to_rgba8())
}

/// After a restart, pick up the newest capture left in the cache dir.
pub(crate) fn restore(app: &AppHandle) {
    let newest = dir(app)
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "png")
                && !e.file_name().to_string_lossy().starts_with('.')
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    *app.state::<Latest>().0.lock().unwrap() = newest.map(|e| e.path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_looks_like_a_macos_screenshot_name() {
        let s = local_stamp(); // 2026-11-05 at 15.04.22
        let b = s.as_bytes();
        assert_eq!(s.len(), 22, "{s}");
        assert!(
            b[4] == b'-' && b[7] == b'-' && &s[10..14] == " at " && b[16] == b'.' && b[19] == b'.',
            "{s}"
        );
    }

    #[test]
    fn png_round_trip_and_atomic_rename() {
        let dir = std::env::temp_dir().join(format!("scrme-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = RgbaImage::from_fn(37, 21, |x, y| {
            image::Rgba([(x * 6) as u8, (y * 11) as u8, 200, 255])
        });
        let (path, tmp) = (dir.join("a.png"), dir.join(".writing.png"));
        encode(&img, &path, &tmp).unwrap();
        assert!(!tmp.exists(), "temp file is renamed away");
        assert_eq!(load(&path).unwrap(), img);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
