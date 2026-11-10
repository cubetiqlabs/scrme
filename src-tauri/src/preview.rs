//! The floating window shown after a capture (a small preview) and the persistent pin:
//! what its footer buttons do. The window itself is native (`platform::pin_macos`).
//!
//! A preview holds only a small thumbnail. Edit / Copy / Save as / drag-out read the full
//! capture from the PNG that `store` keeps on disk, so nothing big stays in memory.

use std::sync::Arc;

use image::RgbaImage;

use crate::host::Host;
#[cfg(not(target_os = "macos"))]
use crate::toast::{self, Kind};

/// Hand a cropped image to the editor: the buffer moves into `state`, the engine opens
/// (or focuses) its editor window at this size.
pub(crate) fn open_editor_with(img: RgbaImage) {
    let (w, h) = img.dimensions();
    crate::state::set_pending(img);
    crate::host::host().open_editor(w, h);
}

/// After a capture: a small preview in the corner (auto-dismisses unless hovered). The
/// capture itself must already be (being) written as the latest capture (`store`).
pub(crate) fn show_preview(host: &Arc<dyn Host>, img: &RgbaImage) {
    #[cfg(target_os = "macos")]
    macos::present(host.clone(), img, macos::Source::Latest);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = img;
        toast::show(host, Kind::Ok, "Copied");
    }
}

/// A persistent pin (editor's Pin button). The user asked to keep it, so it owns its image.
/// Used by the Tauri editor's Pin button; the GPUI editor (phase 3) will use it too.
#[cfg_attr(not(feature = "tauri-app"), allow(dead_code))]
pub(crate) fn show_pin(host: &Arc<dyn Host>, img: RgbaImage) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let img = Arc::new(img);
        macos::present(host.clone(), &img, macos::Source::Mem(img.clone()));
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (host, img);
        Err("Pin to screen is only available on macOS for now".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;
    use std::sync::Arc;

    use image::RgbaImage;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
    use objc2_foundation::NSString;

    use crate::host::Host;
    use crate::platform::pin_macos::{self, Action, Options};
    use crate::store;
    use crate::toast;

    const THUMB_LONG_EDGE: u32 = 720;
    const PREVIEW_BOX: (f64, f64) = (300.0, 210.0);
    const PREVIEW_SECS: f64 = 8.0;

    /// Where a window's full-size image comes from.
    #[derive(Clone)]
    pub(super) enum Source {
        /// The newest capture, kept on disk by `store` (previews).
        Latest,
        /// An image the user pinned on purpose (kept in memory).
        #[cfg_attr(not(feature = "tauri-app"), allow(dead_code))]
        Mem(Arc<RgbaImage>),
    }

    impl Source {
        fn file(&self) -> Option<PathBuf> {
            match self {
                Source::Latest => store::latest_path(),
                Source::Mem(img) => store::write_scratch(img),
            }
        }

        fn image(&self) -> Option<RgbaImage> {
            match self {
                Source::Latest => store::latest_path().and_then(|p| store::load(&p)),
                Source::Mem(img) => Some((**img).clone()),
            }
        }
    }

    /// Fast, decent-quality downscale: pick every n-th pixel until the image is ~2x the target,
    /// then box-filter the rest. Touching only ~1.4 M pixels instead of a full 7 M-pixel retina
    /// screenshot keeps the preview snappy even for full-screen captures.
    pub(super) fn thumbnail(img: &RgbaImage, long_edge: u32) -> RgbaImage {
        let (w, h) = img.dimensions();
        let k = (long_edge as f64 / w.max(h) as f64).min(1.0);
        let (tw, th) = (
            ((w as f64 * k) as u32).max(1),
            ((h as f64 * k) as u32).max(1),
        );
        let step = (w / (tw * 2)).max(1);
        if step == 1 {
            return image::imageops::thumbnail(img, tw, th);
        }
        let (mw, mh) = (w / step, h / step);
        let src = img.as_raw();
        let mut mid = Vec::with_capacity(mw as usize * mh as usize * 4);
        for y in 0..mh {
            let row = (y * step) as usize * w as usize * 4;
            for x in 0..mw {
                let i = row + (x * step) as usize * 4;
                mid.extend_from_slice(&src[i..i + 4]);
            }
        }
        let mid = RgbaImage::from_raw(mw, mh, mid).expect("sized above");
        image::imageops::thumbnail(&mid, tw, th)
    }

    pub(super) fn present(host: Arc<dyn Host>, img: &RgbaImage, source: Source) {
        let preview = matches!(source, Source::Latest);
        let (w, h) = img.dimensions();
        // The preview only needs a small picture; the full image is read from disk on demand.
        let tiff = if preview {
            crate::tiff::rgb_tiff(&thumbnail(img, THUMB_LONG_EDGE))
        } else {
            crate::tiff::rgb_tiff(img)
        };
        let cb_host = host.clone();
        let shown = host.on_main(Box::new(move || {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            if preview {
                pin_macos::dismiss_previews(); // a new capture replaces the old preview
            }
            let drag = preview.then(|| {
                let source = source.clone();
                Box::new(move || source.file()) as Box<dyn Fn() -> Option<PathBuf>>
            });
            let opts = if preview {
                Options {
                    auto_dismiss: Some(PREVIEW_SECS),
                    max_points: Some(PREVIEW_BOX),
                    corner: true,
                    drag,
                }
            } else {
                Options {
                    auto_dismiss: None,
                    max_points: None,
                    corner: false,
                    drag: None,
                }
            };
            let cb_source = source.clone();
            let act_host = cb_host.clone();
            pin_macos::show(mtm, tiff, (w, h), opts, move |action| {
                act(&act_host, &cb_source, action, mtm)
            });
            if preview {
                // Building the editor costs a moment: warm it after the preview is visible so
                // "Edit" opens instantly without delaying the preview.
                let warm = cb_host.clone();
                std::thread::spawn(move || warm.prewarm_editor());
            }
        }));
        let _ = shown;
    }

    fn act(host: &Arc<dyn Host>, source: &Source, action: Action, mtm: MainThreadMarker) {
        match action {
            Action::Close => {}
            Action::Copy => {
                let (host, source) = (host.clone(), source.clone());
                std::thread::spawn(move || match source.image() {
                    Some(img) => {
                        host.copy_image(&img);
                        toast::ok(&host, "Copied");
                    }
                    None => toast::err(&host, "Capture not found"),
                });
            }
            Action::Edit => {
                let (host, source) = (host.clone(), source.clone());
                std::thread::spawn(move || match source.image() {
                    Some(img) => super::open_editor_with(img),
                    None => toast::err(&host, "Capture not found"),
                });
            }
            Action::SaveAs => save_as(host, source, mtm),
        }
    }

    /// Native "Save as…" panel (we are on the main thread: it is called from a button click).
    /// The capture is already a PNG on disk, so saving is a file copy (no re-encode).
    fn save_as(host: &Arc<dyn Host>, source: &Source, mtm: MainThreadMarker) {
        crate::platform::activate_app();
        let panel = NSSavePanel::savePanel(mtm);
        let name = source
            .file()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "scrme.png".into());
        panel.setNameFieldStringValue(&NSString::from_str(&name));
        if panel.runModal() != NSModalResponseOK {
            return;
        }
        let Some(dest) = panel.URL().and_then(|u| u.path()).map(|p| p.to_string()) else {
            return;
        };
        let saved = source
            .file()
            .is_some_and(|f| std::fs::copy(f, &dest).is_ok());
        if saved {
            toast::ok(host, "Saved");
        } else {
            toast::err(host, "Save failed");
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos::thumbnail;
    use image::RgbaImage;

    #[test]
    fn thumbnail_keeps_aspect_and_averages() {
        // 1000x500 split into a black left half and a white right half
        let img = RgbaImage::from_fn(1000, 500, |x, _| {
            if x < 500 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        });
        let t = thumbnail(&img, 100);
        assert_eq!(t.dimensions(), (100, 50));
        assert_eq!(t.get_pixel(10, 25)[0], 0);
        assert_eq!(t.get_pixel(90, 25)[0], 255);
        // a source already smaller than the target is returned at its own size
        assert_eq!(
            thumbnail(&RgbaImage::new(40, 20), 100).dimensions(),
            (40, 20)
        );
    }
}
