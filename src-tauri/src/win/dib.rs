//! CF_DIB decoding: BI_BITFIELDS channel masks, packed or headered DIBs.

fn read_i32(dib: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([dib[off], dib[off + 1], dib[off + 2], dib[off + 3]])
}

/// DIB (packed or with header) → RGBA → PNG bytes.
pub fn dib_to_png(dib: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if dib.len() < 40 {
        return None;
    }
    let width = read_i32(dib, 4) as u32;
    let height = read_i32(dib, 8);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    if width == 0 || width > 20000 || height.unsigned_abs() > 20000 || (bpp != 24 && bpp != 32) {
        return None;
    }
    let top_down = height < 0;
    let h = height.unsigned_abs();
    let stride = (width as usize * bpp as usize).div_ceil(32) * 4;
    let header_size = read_i32(dib, 0) as usize;
    let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);
    // BI_BITFIELDS (3): channel masks follow a 40-byte header, or live inside
    // a BITMAPV4/V5 header (108/124 bytes) at offset 40
    let bitfields = compression == 3;
    let data_off = if bitfields && header_size <= 40 {
        40 + 12
    } else {
        header_size.max(40)
    };
    if data_off > dib.len() {
        return None;
    }
    // default BGRA masks; overridden by explicit bitfields
    let mut masks: [u32; 4] = [0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000]; // R,G,B,A
    if bitfields {
        // classic 40-byte header carries only 3 masks (RGB); the 4th (alpha)
        // exists only in BITMAPV4/V5 headers — reading it from pixel data
        // corrupts alpha
        let mask_count = if header_size >= 56 { 4 } else { 3 };
        for (slot, mask_out) in masks.iter_mut().enumerate().take(mask_count) {
            let o = 40 + slot * 4;
            if o + 4 <= dib.len() {
                let m = u32::from_le_bytes([dib[o], dib[o + 1], dib[o + 2], dib[o + 3]]);
                if m != 0 {
                    *mask_out = m;
                }
            }
        }
    }
    let decode = |v: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 255;
        }
        let bits = mask.count_ones();
        if bits == 0 || bits >= 32 {
            return 255;
        }
        let shift = mask.trailing_zeros();
        let max = (1u64 << bits) - 1;
        let val = (((v & mask) >> shift) as u64 * 255 + max / 2) / max;
        val as u8
    };
    let px = &dib[data_off..];
    let mut buf = vec![0u8; width as usize * h as usize * 4];
    for y in 0..h as usize {
        let src_y = if top_down { y } else { h as usize - 1 - y };
        for x in 0..width as usize {
            let si = src_y * stride + x * (bpp as usize / 8);
            let di = (y * width as usize + x) * 4;
            if si + (bpp as usize / 8) > px.len() {
                continue;
            }
            let (r, g, b, a) = if bpp == 32 {
                let v = u32::from_le_bytes([px[si], px[si + 1], px[si + 2], px[si + 3]]);
                (
                    decode(v, masks[0]),
                    decode(v, masks[1]),
                    decode(v, masks[2]),
                    decode(v, masks[3]),
                )
            } else {
                (px[si + 2], px[si + 1], px[si], 255u8)
            };
            buf[di] = r;
            buf[di + 1] = g;
            buf[di + 2] = b;
            buf[di + 3] = if a == 0 { 255 } else { a };
        }
    }
    let img = image::RgbaImage::from_raw(width, h, buf)?;
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some((png.into_inner(), width, h))
}
