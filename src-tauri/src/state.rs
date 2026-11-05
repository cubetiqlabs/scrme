use std::sync::Mutex;

use image::RgbaImage;

/// Frozen frame of the monitor, grabbed at hotkey time. Never leaves Rust:
/// the overlay only draws the selection, the crop happens here.
#[derive(Default)]
pub(crate) struct Frozen(pub(crate) Mutex<Option<RgbaImage>>);

/// Cropped image waiting to be loaded by the editor window.
#[derive(Default)]
pub(crate) struct Pending(pub(crate) Mutex<Option<RgbaImage>>);
