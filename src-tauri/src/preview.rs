//! The floating window shown after a capture (a small preview) and the persistent pin:
//! what its footer buttons do. The window itself is native (`platform::pin_macos`).

use std::sync::Arc;

use image::RgbaImage;
use tauri::AppHandle;

#[cfg(not(target_os = "macos"))]
use crate::toast::show_toast;

/// After a capture: a small preview in the corner (auto-dismisses unless hovered).
pub(crate) fn show_preview(app: &AppHandle, img: Arc<RgbaImage>) {
    #[cfg(target_os = "macos")]
    macos::present(app, img, true);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = img;
        show_toast(app, "Copied");
    }
}

/// A persistent pin (editor's Pin button).
pub(crate) fn show_pin(app: &AppHandle, img: RgbaImage) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::present(app, Arc::new(img), false);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, img);
        Err("Pin to screen is only available on macOS for now".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Arc;

    use image::RgbaImage;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
    use objc2_foundation::NSString;
    use tauri::AppHandle;

    use crate::commands::open_editor_with;
    use crate::output::copy_image;
    use crate::platform::pin_macos::{self, Action, Options};
    use crate::toast::{show_toast, show_toast_err};
    use crate::windows::activate_app;

    const THUMB_LONG_EDGE: u32 = 720;
    const PREVIEW_BOX: (f64, f64) = (300.0, 210.0);
    const PREVIEW_SECS: f64 = 8.0;

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

    pub(super) fn present(app: &AppHandle, img: Arc<RgbaImage>, preview: bool) {
        let (w, h) = img.dimensions();
        // The preview only needs a small picture; the full image stays in memory for Edit/Copy/Save.
        let tiff = if preview {
            crate::tiff::rgb_tiff(&thumbnail(&img, THUMB_LONG_EDGE))
        } else {
            crate::tiff::rgb_tiff(&img)
        };
        let opts = if preview {
            Options {
                auto_dismiss: Some(PREVIEW_SECS),
                max_points: Some(PREVIEW_BOX),
                corner: true,
            }
        } else {
            Options {
                auto_dismiss: None,
                max_points: None,
                corner: false,
            }
        };
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            if preview {
                pin_macos::dismiss_previews(); // a new capture replaces the old preview
            }
            let app = handle.clone();
            pin_macos::show(mtm, tiff, (w, h), opts, move |action| {
                act(&app, &img, action, mtm)
            });
            if preview {
                // Building a webview takes a while: do it after the preview is visible, off the
                // main thread, so "Edit" opens instantly without delaying the preview.
                let app = handle.clone();
                std::thread::spawn(move || crate::windows::prewarm_editor(&app));
            }
        });
    }

    fn act(app: &AppHandle, img: &Arc<RgbaImage>, action: Action, mtm: MainThreadMarker) {
        match action {
            Action::Close => {}
            Action::Copy => {
                copy_image(app, img);
                show_toast(app, "Copied");
            }
            Action::Edit => {
                let (app, img) = (app.clone(), img.clone());
                std::thread::spawn(move || open_editor_with(&app, (*img).clone()));
            }
            Action::SaveAs => save_as(app, img, mtm),
        }
    }

    /// Native "Save as…" panel (we are on the main thread: it is called from a button click).
    fn save_as(app: &AppHandle, img: &Arc<RgbaImage>, mtm: MainThreadMarker) {
        activate_app();
        let panel = NSSavePanel::savePanel(mtm);
        panel.setNameFieldStringValue(&NSString::from_str("scrme.png"));
        if panel.runModal() != NSModalResponseOK {
            return;
        }
        let Some(path) = panel.URL().and_then(|u| u.path()).map(|p| p.to_string()) else {
            return;
        };
        match img.save(&path) {
            Ok(()) => show_toast(app, "Saved"),
            Err(e) => {
                eprintln!("save failed: {e}");
                show_toast_err(app, "Save failed");
            }
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
