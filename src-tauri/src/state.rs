//! Process-wide state shared by the capture path. Small and lock-guarded so a worker
//! thread can finish a capture while the UI thread is free.

use std::sync::{Mutex, MutexGuard};

use image::RgbaImage;

use crate::capture::Frame;

/// Frozen frame of the monitor, grabbed at hotkey time. Never leaves Rust:
/// the overlay only draws the selection, the crop happens here.
static FROZEN: Mutex<Option<Frame>> = Mutex::new(None);

/// Cropped image waiting to be loaded by the editor window.
static PENDING: Mutex<Option<RgbaImage>> = Mutex::new(None);

fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    // A panic while holding one of these leaves a plain Option, so recovering is safe.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_frozen(frame: Frame) {
    *lock(&FROZEN) = Some(frame);
}

pub fn take_frozen() -> Option<Frame> {
    lock(&FROZEN).take()
}

pub fn clear_frozen() {
    lock(&FROZEN).take();
}

pub fn set_pending(img: RgbaImage) {
    *lock(&PENDING) = Some(img);
}

pub fn take_pending() -> Option<RgbaImage> {
    lock(&PENDING).take()
}

#[cfg_attr(not(feature = "tauri-app"), allow(dead_code))]
pub fn pending_size() -> Option<(u32, u32)> {
    lock(&PENDING).as_ref().map(|i| i.dimensions())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_round_trip_hands_the_buffer_over_once() {
        set_pending(RgbaImage::new(4, 3));
        assert_eq!(pending_size(), Some((4, 3)));
        let taken = take_pending().expect("stored");
        assert_eq!(taken.dimensions(), (4, 3));
        assert!(
            take_pending().is_none(),
            "the buffer moves out exactly once"
        );
        assert!(pending_size().is_none());
    }
}
