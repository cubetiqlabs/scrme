//! macOS capture backend: keep the screenshot as a `CGImage` and render only the
//! requested crop into an RGBA buffer through a `CGBitmapContext`.
//!
//! This replaces xcap's path (CFData copy → `to_vec` → row-strip copy → BGRA swap
//! loop), which touches ~3 full retina frames per capture.

use image::RgbaImage;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGBitmapInfo, CGDataProvider, CGDisplayBounds, CGImage, CGWindowImageOption, CGWindowListOption,
};
// ponytail: deprecated since macOS 15 but still what xcap uses and what works on 27;
// migrate to ScreenCaptureKit (SCScreenshotManager) when it stops working.
#[allow(deprecated)]
use objc2_core_graphics::CGWindowListCreateImage;

pub(crate) struct CgFrame {
    image: CFRetained<CGImage>,
    w: u32,
    h: u32,
}

// SAFETY: a CGImage is an immutable, reference-counted Core Graphics object that
// Apple documents as safe to use from any thread.
unsafe impl Send for CgFrame {}

/// Snapshot what is on screen for `display_id` (a CGDirectDisplayID, e.g. `xcap::Monitor::id`).
#[allow(deprecated)]
pub(crate) fn freeze(display_id: u32) -> Option<CgFrame> {
    let bounds = CGDisplayBounds(display_id);
    let image = CGWindowListCreateImage(
        bounds,
        CGWindowListOption::OptionOnScreenOnly,
        0, // kCGNullWindowID: all windows
        CGWindowImageOption::Default,
    )?;
    let w = CGImage::width(Some(&image)) as u32;
    let h = CGImage::height(Some(&image)) as u32;
    // A denied Screen Recording permission yields an empty/1px image, not an error.
    // We read the pixels as-is, so only accept the usual BGRA8 layout (else fall back to xcap).
    let info = CGImage::bitmap_info(Some(&image)).bits();
    let little32 =
        info & CGBitmapInfo::ByteOrderMask.bits() == CGBitmapInfo::ByteOrder32Little.bits();
    let alpha_first = matches!(info & CGBitmapInfo::AlphaInfoMask.bits(), 2 | 6); // premul-first / none-skip-first
    let ok =
        w > 1 && h > 1 && CGImage::bits_per_pixel(Some(&image)) == 32 && little32 && alpha_first;
    ok.then_some(CgFrame { image, w, h })
}

impl CgFrame {
    pub(crate) fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// RGBA pixels of the rect (top-left origin, physical px, already clamped).
    ///
    /// Reads the image's own BGRA bytes once (`CGDataProvider` copy, freed on return) and
    /// converts only the requested rows. (A `CGBitmapContext` draw looked cheaper but
    /// permanently retained one output-sized buffer per call.)
    pub(crate) fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Option<RgbaImage> {
        let provider = CGImage::data_provider(Some(&self.image))?;
        let data = CGDataProvider::data(Some(&provider))?;
        let stride = CGImage::bytes_per_row(Some(&self.image));
        // SAFETY: the slice borrows `data`, which outlives every use below.
        let src = unsafe { data.as_bytes_unchecked() };
        let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);
        if src.len() < stride * (y + h) || stride < (x + w) * 4 {
            return None;
        }
        let mut out = Vec::with_capacity(w * h * 4);
        for row in y..y + h {
            let start = row * stride + x * 4;
            out.extend_from_slice(&src[start..start + w * 4]);
        }
        for px in out.chunks_exact_mut(4) {
            px.swap(0, 2); // BGRA -> RGBA
            px[3] = 255; // screenshots are opaque
        }
        RgbaImage::from_raw(w as u32, h as u32, out)
    }
}

#[cfg(test)]
mod tests {
    use super::freeze;
    use std::time::Instant;

    /// Mean absolute per-channel difference between two equally sized RGBA crops.
    fn diff(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
        let sum: u64 = a
            .as_raw()
            .chunks_exact(4)
            .zip(b.as_raw().chunks_exact(4))
            .map(|(p, q)| (0..3).map(|i| p[i].abs_diff(q[i]) as u64).sum::<u64>())
            .sum();
        sum as f64 / (a.width() as f64 * a.height() as f64 * 3.0)
    }

    /// Needs Screen Recording permission for the test runner:
    /// `cargo test --lib capture::macos -- --ignored --nocapture`
    #[test]
    #[ignore = "needs Screen Recording permission and a visible screen"]
    fn crop_matches_xcap_and_is_not_flipped() {
        let mon = xcap::Monitor::all()
            .unwrap()
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .unwrap();
        let t = Instant::now();
        let frame = freeze(mon.id().unwrap()).expect("freeze (permission?)");
        println!("freeze: {:?}  size {:?}", t.elapsed(), frame.size());

        let t = Instant::now();
        let reference = mon.capture_image().unwrap();
        println!("xcap full capture: {:?}", t.elapsed());
        assert_eq!(frame.size(), reference.dimensions());

        let (x, y, w, h) = (200, 300, 400, 300);
        let t = Instant::now();
        let ours = frame.crop(x, y, w, h).unwrap();
        println!("crop {w}x{h}: {:?}", t.elapsed());
        let want = image::imageops::crop_imm(&reference, x, y, w, h).to_image();
        let flipped = image::imageops::flip_vertical(&want);

        let (d, df) = (diff(&ours, &want), diff(&ours, &flipped));
        println!("diff vs xcap {d:.2}, vs vertically flipped xcap {df:.2}");
        assert!(d < 12.0, "crop differs from xcap too much: {d}");
        assert!(d <= df, "crop looks flipped: {d} vs {df}");
        assert!(
            ours.pixels().all(|p| p[3] == 255),
            "screenshots must be opaque"
        );

        let t = Instant::now();
        let full = frame.crop(0, 0, frame.size().0, frame.size().1).unwrap();
        println!(
            "full-frame render: {:?} ({} MB)",
            t.elapsed(),
            full.as_raw().len() >> 20
        );
    }
}

#[cfg(test)]
mod leak_probe {
    use super::freeze;

    fn rss_mb() -> u64 {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<u64>()
            .unwrap_or(0)
            / 1024
    }

    /// Regression: a CGBitmapContext-based crop retained one output-sized buffer per call
    /// (+29 MB per full capture). Capturing repeatedly must not grow the process.
    #[test]
    #[ignore = "needs Screen Recording permission"]
    fn repeated_captures_do_not_accumulate() {
        let id = xcap::Monitor::all()
            .unwrap()
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .unwrap()
            .id()
            .unwrap();
        let one = |full: bool| {
            let f = freeze(id).unwrap();
            let (w, h) = f.size();
            let img = if full {
                f.crop(0, 0, w, h)
            } else {
                f.crop(0, 0, 400, 300)
            };
            drop(img);
        };
        one(true); // warm-up (allocator arenas, CG caches)
        let base = rss_mb();
        for _ in 0..6 {
            one(true);
            one(false);
        }
        let grown = rss_mb().saturating_sub(base);
        println!("rss grew {grown} MB over 12 captures (base {base} MB)");
        assert!(grown < 40, "captures accumulate memory: +{grown} MB");
    }
}
