//! A frozen screenshot. On macOS it stays a `CGImage` (no full-frame RGBA copies);
//! only the pixels that are actually used (the crop) are ever converted to RGBA.

use image::RgbaImage;

pub(crate) enum Frame {
    /// Fallback backend (xcap) and non-macOS platforms.
    Rgba(RgbaImage),
    #[cfg(target_os = "macos")]
    Cg(super::macos::CgFrame),
}

impl Frame {
    /// Size in physical pixels.
    pub(crate) fn size(&self) -> (u32, u32) {
        match self {
            Frame::Rgba(i) => i.dimensions(),
            #[cfg(target_os = "macos")]
            Frame::Cg(c) => c.size(),
        }
    }

    /// RGBA pixels of the (already clamped) rect `x, y, w, h`.
    pub(crate) fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Option<RgbaImage> {
        match self {
            Frame::Rgba(i) => Some(image::imageops::crop_imm(i, x, y, w, h).to_image()),
            #[cfg(target_os = "macos")]
            Frame::Cg(c) => c.crop(x, y, w, h),
        }
    }

    /// The whole frame as RGBA (one buffer, used by full-screen capture).
    pub(crate) fn into_rgba(self) -> Option<RgbaImage> {
        match self {
            Frame::Rgba(i) => Some(i),
            #[cfg(target_os = "macos")]
            Frame::Cg(c) => {
                let (w, h) = c.size();
                c.crop(0, 0, w, h)
            }
        }
    }
}
