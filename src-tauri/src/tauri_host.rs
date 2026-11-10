//! Tauri's implementation of the engine seam.

use image::RgbaImage;
use tauri::AppHandle;

use crate::capture::Frame;
use crate::host::Host;
use crate::output;
use crate::toast::{self, Kind};
use crate::windows;

pub(crate) struct TauriHost {
    app: AppHandle,
}

impl TauriHost {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Host for TauriHost {
    fn on_main(&self, f: Box<dyn FnOnce() + Send>) -> bool {
        self.app.run_on_main_thread(f).is_ok()
    }

    fn toast(&self, kind: Kind, text: &str) {
        toast::show_webview(&self.app, kind, text);
    }

    fn copy_image(&self, img: &RgbaImage) {
        #[cfg(target_os = "macos")]
        output::copy_image(img);
        #[cfg(not(target_os = "macos"))]
        {
            use tauri::image::Image;
            use tauri_plugin_clipboard_manager::ClipboardExt;
            let (w, h) = img.dimensions();
            let _ = self
                .app
                .clipboard()
                .write_image(&Image::new_owned(img.as_raw().clone(), w, h));
        }
    }

    fn open_editor(&self, w: u32, h: u32) {
        windows::open_editor(&self.app, w, h);
    }

    fn prewarm_editor(&self) {
        windows::prewarm_editor(&self.app);
    }

    fn show_web_overlay(&self, frame: Frame, origin: (f64, f64)) -> bool {
        windows::show_web_overlay(&self.app, frame, origin)
    }
}
