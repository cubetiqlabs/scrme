//! Native macOS pasteboard write (see `tiff.rs` for why we don't use arboard here).

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeTIFF};
use objc2_foundation::NSData;

/// Replace the general pasteboard's contents with one TIFF image. `tiff` is moved into
/// the `NSData` (no copy); the pasteboard server keeps its own copy afterwards.
pub(crate) fn set_tiff(tiff: Vec<u8>) -> Result<(), String> {
    autoreleasepool(|_| {
        // SAFETY: plain AppKit calls; NSPasteboard may be used from any thread.
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let data = NSData::from_vec(tiff);
        if unsafe { pb.setData_forType(Some(&data), NSPasteboardTypeTIFF) } {
            Ok(())
        } else {
            Err("pasteboard rejected the image".into())
        }
    })
}
