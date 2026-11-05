use std::sync::Mutex;

use image::RgbaImage;

use crate::capture::Frame;

/// Frozen frame of the monitor, grabbed at hotkey time. Never leaves Rust:
/// the overlay only draws the selection, the crop happens here.
#[derive(Default)]
pub(crate) struct Frozen(pub(crate) Mutex<Option<Frame>>);

/// Set while an area capture started from the "capture & annotate" action: the selection
/// opens in the editor even without holding ⇧.
#[derive(Default)]
pub(crate) struct EditMode(pub(crate) std::sync::atomic::AtomicBool);

/// Cropped image waiting to be loaded by the editor window.
#[derive(Default)]
pub(crate) struct Pending(pub(crate) Mutex<Option<RgbaImage>>);
