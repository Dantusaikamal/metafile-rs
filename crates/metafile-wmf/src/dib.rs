use crate::reader::Reader;
use metafile_core::{Bitmap, MetafileError, ResourceLimits, Result};

pub fn decode_dib(data: &[u8], record_index: usize, limits: &ResourceLimits) -> Result<Bitmap> {
    let bad = |m: &str| MetafileError::InvalidBitmap {
        record_index,
        message: m.into(),
    };
    let mut r = Reader::new(data);
    let header_size = r.u32()?;
    if header_size < 40 {
        return Err(bad(
            "only BITMAPINFOHEADER (40 bytes or larger) is supported",
        ));
    }
    if usize::try_from(header_size).unwrap_or(usize::MAX) > data.len() {
        return Err(bad("DIB header extends past record"));
    }
    let width = r.i32()?;
    let signed_height = r.i32()?;
    let planes = r.u16()?;
    let bpp = r.u16()?;
    let compression = r.u32()?;
    let image_size = r.u32()?;
    r.skip(8)?; // horizontal and vertical pixels-per-meter
    let colors_used = r.u32()?;
    r.skip(4)?; // important color count
    if width <= 0 || signed_height == 0 {
        return Err(bad("invalid dimensions"));
    }
    if planes != 1 {
        return Err(bad("BITMAPINFOHEADER planes must equal 1"));
    }
    let w = u32::try_from(width).map_err(|_| bad("negative width"))?;
    let h = signed_height.unsigned_abs();
    if w > limits.max_dimension || h > limits.max_dimension {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "bitmap dimension",
            actual: u64::from(w.max(h)),
            limit: u64::from(limits.max_dimension),
        });
    }
    let pixels = u64::from(w)
        .checked_mul(u64::from(h))
        .ok_or_else(|| bad("pixel count overflow"))?;
    if pixels > limits.max_pixels {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "bitmap pixels",
            actual: pixels,
            limit: limits.max_pixels,
        });
    }
    if compression != 0 {
        return Err(MetafileError::UnsupportedBitmap {
            record_index,
            message: format!("DIB compression {compression} is not supported"),
        });
    }
    if !matches!(bpp, 1 | 4 | 8 | 24 | 32) {
        return Err(MetafileError::UnsupportedBitmap {
            record_index,
            message: format!("{bpp}-bit DIB pixels are not supported"),
        });
    }
    if bpp <= 8 && colors_used > (1u32 << bpp) {
        return Err(bad("colors_used exceeds the indexed pixel range"));
    }
    let palette_count = if bpp <= 8 {
        if colors_used == 0 {
            1u32 << bpp
        } else {
            colors_used
        }
    } else {
        0
    };
    let palette_bytes = usize::try_from(palette_count)
        .ok()
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| bad("palette overflow"))?;
    let mask_bytes = 0;
    let data_offset = usize::try_from(header_size)
        .ok()
        .and_then(|n| n.checked_add(mask_bytes))
        .and_then(|n| n.checked_add(palette_bytes))
        .ok_or_else(|| bad("data offset overflow"))?;
    if data_offset > data.len() {
        return Err(bad("palette or masks extend past record"));
    }
    let row_bits = usize::from(bpp)
        .checked_mul(w as usize)
        .ok_or_else(|| bad("row size overflow"))?;
    let stride = row_bits
        .checked_add(31)
        .map(|v| (v / 32) * 4)
        .ok_or_else(|| bad("row size overflow"))?;
    let needed = stride
        .checked_mul(h as usize)
        .ok_or_else(|| bad("bitmap size overflow"))?;
    if data.len() - data_offset < needed {
        return Err(bad("truncated pixel data"));
    }
    if image_size != 0 && u64::from(image_size) < needed as u64 {
        return Err(bad(
            "declared image size is smaller than required pixel data",
        ));
    }
    let palette = &data[usize::try_from(header_size).unwrap() + mask_bytes..data_offset];
    let src = &data[data_offset..data_offset + needed];
    let cap = (pixels * 4) as usize;
    let mut rgba = vec![0u8; cap];
    for y in 0..h as usize {
        let sy = if signed_height > 0 {
            h as usize - 1 - y
        } else {
            y
        };
        let row = &src[sy * stride..(sy + 1) * stride];
        for x in 0..w as usize {
            let (red, green, blue) = match bpp {
                32 => (row[x * 4 + 2], row[x * 4 + 1], row[x * 4]),
                24 => (row[x * 3 + 2], row[x * 3 + 1], row[x * 3]),
                8 => pal(palette, usize::from(row[x]), record_index)?,
                4 => {
                    let v = row[x / 2];
                    pal(
                        palette,
                        usize::from(if x % 2 == 0 { v >> 4 } else { v & 15 }),
                        record_index,
                    )?
                }
                1 => {
                    let v = (row[x / 8] >> (7 - (x % 8))) & 1;
                    pal(palette, usize::from(v), record_index)?
                }
                _ => unreachable!(),
            };
            let o = (y * w as usize + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[red, green, blue, 255]);
        }
    }
    Ok(Bitmap {
        width: w,
        height: h,
        rgba,
    })
}
fn pal(p: &[u8], i: usize, record_index: usize) -> Result<(u8, u8, u8)> {
    let o = i
        .checked_mul(4)
        .ok_or_else(|| MetafileError::InvalidBitmap {
            record_index,
            message: "palette index overflow".into(),
        })?;
    if o + 3 >= p.len() {
        return Err(MetafileError::InvalidBitmap {
            record_index,
            message: "palette index out of bounds".into(),
        });
    }
    Ok((p[o + 2], p[o + 1], p[o]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_bottom_up_24bit() {
        let mut d = vec![0u8; 48];
        d[0..4].copy_from_slice(&40u32.to_le_bytes());
        d[4..8].copy_from_slice(&2i32.to_le_bytes());
        d[8..12].copy_from_slice(&1i32.to_le_bytes());
        d[12..14].copy_from_slice(&1u16.to_le_bytes());
        d[14..16].copy_from_slice(&24u16.to_le_bytes());
        d[40..46].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
        let b = decode_dib(&d, 0, &ResourceLimits::default()).unwrap();
        assert_eq!(&b.rgba[..8], &[255, 0, 0, 255, 0, 255, 0, 255]);
    }
    #[test]
    fn rejects_truncated() {
        assert!(decode_dib(&[40, 0, 0, 0], 2, &ResourceLimits::default()).is_err());
    }
    #[test]
    fn validates_planes_palette_and_image_size() {
        let base = || {
            let mut d = vec![0u8; 44];
            d[0..4].copy_from_slice(&40u32.to_le_bytes());
            d[4..8].copy_from_slice(&1i32.to_le_bytes());
            d[8..12].copy_from_slice(&1i32.to_le_bytes());
            d[12..14].copy_from_slice(&1u16.to_le_bytes());
            d[14..16].copy_from_slice(&32u16.to_le_bytes());
            d
        };
        let mut planes = base();
        planes[12..14].copy_from_slice(&2u16.to_le_bytes());
        assert!(matches!(
            decode_dib(&planes, 1, &ResourceLimits::default()),
            Err(MetafileError::InvalidBitmap { .. })
        ));

        let mut image_size = base();
        image_size[20..24].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            decode_dib(&image_size, 1, &ResourceLimits::default()),
            Err(MetafileError::InvalidBitmap { .. })
        ));

        let mut palette = vec![0u8; 48];
        palette[0..4].copy_from_slice(&40u32.to_le_bytes());
        palette[4..8].copy_from_slice(&1i32.to_le_bytes());
        palette[8..12].copy_from_slice(&1i32.to_le_bytes());
        palette[12..14].copy_from_slice(&1u16.to_le_bytes());
        palette[14..16].copy_from_slice(&1u16.to_le_bytes());
        palette[32..36].copy_from_slice(&3u32.to_le_bytes());
        assert!(matches!(
            decode_dib(&palette, 1, &ResourceLimits::default()),
            Err(MetafileError::InvalidBitmap { .. })
        ));
    }
}
