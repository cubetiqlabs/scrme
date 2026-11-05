//! Minimal uncompressed RGB TIFF writer, used for the macOS pasteboard.
//!
//! `arboard`'s image write retained ~1-2 output-sized buffers per copy (measured: +29..58 MB
//! per full-screen copy, never returned), so on macOS we hand the pasteboard a TIFF we built
//! ourselves: one allocation (header + pixels) that is moved, not copied, into an `NSData`.

use image::RgbaImage;

const IFD_ENTRIES: u16 = 10;
const BITS_OFFSET: u32 = 8 + 2 + IFD_ENTRIES as u32 * 12 + 4; // after header, IFD, next-IFD pointer
const DATA_OFFSET: u32 = BITS_OFFSET + 6; // BitsPerSample = 3 shorts

fn entry(buf: &mut Vec<u8>, tag: u16, typ: u16, count: u32, value: u32) {
    buf.extend_from_slice(&tag.to_le_bytes());
    buf.extend_from_slice(&typ.to_le_bytes());
    buf.extend_from_slice(&count.to_le_bytes());
    if typ == 3 && count == 1 {
        // SHORT values are left-justified in the 4-byte value field
        buf.extend_from_slice(&(value as u16).to_le_bytes());
        buf.extend_from_slice(&[0, 0]);
    } else {
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

/// Opaque screenshot as an uncompressed 8-bit RGB TIFF (alpha is dropped).
pub(crate) fn rgb_tiff(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let pixels = w as usize * h as usize * 3;
    let mut out = Vec::with_capacity(DATA_OFFSET as usize + pixels);
    out.extend_from_slice(b"II*\0");
    out.extend_from_slice(&8u32.to_le_bytes()); // first IFD right after the header
    out.extend_from_slice(&IFD_ENTRIES.to_le_bytes());
    entry(&mut out, 256, 4, 1, w); // ImageWidth
    entry(&mut out, 257, 4, 1, h); // ImageLength
    entry(&mut out, 258, 3, 3, BITS_OFFSET); // BitsPerSample -> 8,8,8
    entry(&mut out, 259, 3, 1, 1); // Compression: none
    entry(&mut out, 262, 3, 1, 2); // Photometric: RGB
    entry(&mut out, 273, 4, 1, DATA_OFFSET); // StripOffsets
    entry(&mut out, 277, 3, 1, 3); // SamplesPerPixel
    entry(&mut out, 278, 4, 1, h); // RowsPerStrip: one strip
    entry(&mut out, 279, 4, 1, pixels as u32); // StripByteCounts
    entry(&mut out, 284, 3, 1, 1); // PlanarConfiguration: chunky
    out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    for _ in 0..3 {
        out.extend_from_slice(&8u16.to_le_bytes());
    }
    debug_assert_eq!(out.len() as u32, DATA_OFFSET);
    let (pixels, _) = img.as_raw().as_chunks::<4>();
    for px in pixels {
        out.extend_from_slice(&px[..3]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }
    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }

    #[test]
    fn header_tags_and_pixels() {
        let img = RgbaImage::from_raw(2, 1, vec![10, 20, 30, 255, 40, 50, 60, 255]).unwrap();
        let t = rgb_tiff(&img);
        assert_eq!(&t[..4], b"II*\0");
        assert_eq!(u32_at(&t, 4), 8);
        assert_eq!(u16_at(&t, 8), IFD_ENTRIES);
        // every entry: (tag, value) in ascending tag order, as the TIFF spec requires
        let tags: Vec<u16> = (0..IFD_ENTRIES as usize)
            .map(|i| u16_at(&t, 10 + i * 12))
            .collect();
        assert_eq!(tags, [256, 257, 258, 259, 262, 273, 277, 278, 279, 284]);
        let value = |tag: u16| {
            let i = (0..IFD_ENTRIES as usize)
                .find(|i| u16_at(&t, 10 + i * 12) == tag)
                .unwrap();
            u32_at(&t, 10 + i * 12 + 8)
        };
        assert_eq!(value(256), 2);
        assert_eq!(value(257), 1);
        assert_eq!(value(273), DATA_OFFSET);
        assert_eq!(value(279), 6);
        // BitsPerSample array and pixel data (alpha dropped)
        assert_eq!(
            &t[BITS_OFFSET as usize..DATA_OFFSET as usize],
            [8, 0, 8, 0, 8, 0]
        );
        assert_eq!(&t[DATA_OFFSET as usize..], [10, 20, 30, 40, 50, 60]);
    }
}
