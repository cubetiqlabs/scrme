//! The latest capture lives on disk, not in memory: a PNG in the app cache dir
//! (`~/Library/Caches/<id>/captures/`). The preview's Edit / Copy / Save as and drag-out all
//! read it from there, and "Last Capture" in the tray reopens it later, even after the
//! preview closed and the clipboard was overwritten. Only the newest file is kept.

use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use image::RgbaImage;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};

/// Where the newest capture lives. The mutex is held while a new one is being written,
/// so anything asking for "the latest" waits until its file really exists.
struct Store {
    dir: PathBuf,
    latest: Mutex<Option<PathBuf>>,
}

static STORE: OnceLock<Store> = OnceLock::new();

/// Point the store at `dir` and pick up the newest capture already there (after a restart).
pub fn init(dir: PathBuf) {
    let store = STORE.get_or_init(|| Store {
        dir,
        latest: Mutex::new(None),
    });
    store.restore();
}

fn store() -> Option<&'static Store> {
    STORE.get()
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

/// Hold the "latest" slot while the new capture is being shown and written, so the
/// preview's buttons never read the previous capture.
pub struct WriteGuard {
    store: &'static Store,
    slot: MutexGuard<'static, Option<PathBuf>>,
}

impl WriteGuard {
    /// Write `img` as the new latest capture and record it.
    pub fn put(&mut self, img: &RgbaImage) -> Option<PathBuf> {
        let path = write_png(&self.store.dir, img, self.slot.take())?;
        *self.slot = Some(path.clone());
        Some(path)
    }

    pub fn path(&self) -> Option<&Path> {
        self.slot.as_deref()
    }
}

pub fn lock() -> Option<WriteGuard> {
    let store = store()?;
    let slot = store.latest.lock().unwrap_or_else(|e| e.into_inner());
    Some(WriteGuard { store, slot })
}

/// Write `img` as the new latest capture (fast PNG encode), replacing `previous`.
fn write_png(dir: &Path, img: &RgbaImage, previous: Option<PathBuf>) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("scrme {}.png", local_stamp()));
    encode(img, &path, &dir.join(".writing.png"))?;
    if let Some(old) = previous.filter(|p| *p != path) {
        let _ = std::fs::remove_file(old);
    }
    Some(path)
}

/// Make `img` the newest capture, replacing the previous file.
#[cfg_attr(not(feature = "tauri-app"), allow(dead_code))]
pub fn replace_latest(img: &RgbaImage) -> Option<PathBuf> {
    lock()?.put(img)
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
pub fn write_scratch(img: &RgbaImage) -> Option<PathBuf> {
    let dir = &store()?.dir;
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("pin {}.png", local_stamp()));
    encode(img, &path, &dir.join(".scratch.png"))?;
    Some(path)
}

/// The newest capture's file, once any write in progress has finished.
pub fn latest_path() -> Option<PathBuf> {
    let store = store()?;
    let path = store
        .latest
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    path.filter(|p| p.exists())
}

pub fn load(path: &Path) -> Option<RgbaImage> {
    image::open(path).ok().map(|i| i.to_rgba8())
}

impl Store {
    /// After a restart, pick up the newest capture left in the cache dir.
    fn restore(&self) {
        let newest = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.path().extension().is_some_and(|x| x == "png")
                    && !e.file_name().to_string_lossy().starts_with('.')
            })
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = newest.map(|e| e.path());
    }
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
        let dir =
            std::env::temp_dir().join(format!("scrme-test-{}-{}", std::process::id(), line!()));
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

    #[test]
    fn write_png_replaces_the_previous_file() {
        let dir = std::env::temp_dir().join(format!("scrme-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let stale = dir.join("stale.png");
        std::fs::write(&stale, b"old capture").unwrap();
        let img = RgbaImage::from_pixel(2, 2, image::Rgba([9, 8, 7, 255]));
        let path = write_png(&dir, &img, Some(stale.clone())).unwrap();
        assert!(!stale.exists(), "the previous capture is removed");
        assert_eq!(load(&path).unwrap(), img);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
