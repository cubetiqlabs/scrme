//! The floating window shown after a capture (a small preview) and the persistent pin:
//! what its footer buttons do. The window itself is native (`platform::pin_macos`).

use image::RgbaImage;
use tauri::AppHandle;

#[cfg(not(target_os = "macos"))]
use crate::toast::show_toast;

/// After a capture: a small preview in the corner (auto-dismisses unless hovered).
pub(crate) fn show_preview(app: &AppHandle, img: RgbaImage) {
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
        macos::present(app, img, false);
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

    pub(super) fn present(app: &AppHandle, img: RgbaImage, preview: bool) {
        let (w, h) = img.dimensions();
        // The preview only needs a small picture; the full image stays in memory for Edit/Copy/Save.
        let tiff = if preview {
            let k = (THUMB_LONG_EDGE as f64 / w.max(h) as f64).min(1.0);
            let (tw, th) = (
                ((w as f64 * k) as u32).max(1),
                ((h as f64 * k) as u32).max(1),
            );
            crate::tiff::rgb_tiff(&image::imageops::thumbnail(&img, tw, th))
        } else {
            crate::tiff::rgb_tiff(&img)
        };
        let img = Arc::new(img);
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
                crate::windows::prewarm_editor(&handle); // so "Edit" opens instantly
            }
            let app = handle.clone();
            pin_macos::show(mtm, tiff, (w, h), opts, move |action| {
                act(&app, &img, action, mtm)
            });
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
