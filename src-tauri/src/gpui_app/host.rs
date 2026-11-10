//! GPUI's implementation of the engine seam: main-thread dispatch, the native clipboard and
//! the interim "no editor yet" path.

use image::RgbaImage;

use crate::capture::Frame;
use crate::host::Host;
use crate::output;
use crate::state;
use crate::toast::{self, Kind};

pub(crate) struct GpuiHost;

impl GpuiHost {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Host for GpuiHost {
    fn on_main(&self, f: Box<dyn FnOnce() + Send>) -> bool {
        // The NSApp run loop drains the main queue; GPUI's window work is already there.
        dispatch2::DispatchQueue::main().exec_async(f);
        true
    }

    fn toast(&self, kind: Kind, text: &str) {
        // The native Core Animation pill normally draws first; this only runs when it can't.
        let kind = match kind {
            Kind::Ok => "ok",
            Kind::Err => "error",
        };
        eprintln!("{kind}: {text}");
    }

    fn copy_image(&self, img: &RgbaImage) {
        output::copy_image(img);
    }

    /// Phase 2 has no GPUI canvas editor yet: copy the capture instead of losing it, and say
    /// so. Phase 3 replaces this with the annotation editor (`src/editor` ported to GPUI).
    fn open_editor(&self, _w: u32, _h: u32) {
        if let Some(img) = state::take_pending() {
            self.copy_image(&img);
            toast::ok_now("Copied · editor coming in phase 3");
        }
    }

    fn show_web_overlay(&self, _frame: Frame, _origin: (f64, f64)) -> bool {
        false // the native macOS overlay is the only one this engine has
    }
}
