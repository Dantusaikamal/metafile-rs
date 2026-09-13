use metafile_core::{Bitmap, MetafileError, ResourceLimits, Result};

#[derive(Clone, Copy)]
struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let start = self.position;
        let end = start
            .checked_add(count)
            .ok_or(MetafileError::TruncatedInput {
                offset: start,
                needed: count,
                available: 0,
            })?;
        if end > self.data.len() {
            return Err(MetafileError::TruncatedInput {
                offset: start,
                needed: count,
                available: self.data.len().saturating_sub(start),
            });
        }
        self.position = end;
        Ok(&self.data[start..end])
    }
    fn skip(&mut self, count: usize) -> Result<()> {
        self.take(count).map(|_| ())
    }
    fn u16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

pub struct DecodedDib {
    pub bitmap: Bitmap,
    pub top_down: bool,
}

pub fn decode_dib(data: &[u8], record_index: usize, limits: &ResourceLimits) -> Result<DecodedDib> {
    let bad = |message: &str| MetafileError::InvalidBitmap {
        record_index,
        message: message.into(),
    };
    let mut reader = Reader::new(data);
    let header_size = reader.u32()?;
    if header_size < 40 {
        return Err(bad(
            "only BITMAPINFOHEADER (40 bytes or larger) is supported",
        ));
    }
    let header_len = usize::try_from(header_size).map_err(|_| bad("DIB header overflow"))?;
    if header_len > data.len() {
        return Err(bad("DIB header extends past record"));
    }
    let width = reader.i32()?;
    let signed_height = reader.i32()?;
    let planes = reader.u16()?;
    let bpp = reader.u16()?;
    let compression = reader.u32()?;
    let image_size = reader.u32()?;
    reader.skip(8)?;
    let colors_used = reader.u32()?;
    reader.skip(4)?;
    if width <= 0 || signed_height == 0 {
        return Err(bad("invalid dimensions"));
    }
    if planes != 1 {
        return Err(bad("BITMAPINFOHEADER planes must equal 1"));
    }
    let width = u32::try_from(width).map_err(|_| bad("negative width"))?;
    let height = signed_height.unsigned_abs();
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "bitmap dimension",
            actual: u64::from(width.max(height)),
            limit: u64::from(limits.max_dimension),
        });
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
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
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| bad("palette overflow"))?;
    let data_offset = header_len
        .checked_add(palette_bytes)
        .ok_or_else(|| bad("data offset overflow"))?;
    if data_offset > data.len() {
        return Err(bad("palette extends past record"));
    }
    let row_bits = usize::from(bpp)
        .checked_mul(width as usize)
        .ok_or_else(|| bad("row size overflow"))?;
    let stride = row_bits
        .checked_add(31)
        .map(|value| (value / 32) * 4)
        .ok_or_else(|| bad("row size overflow"))?;
    let needed = stride
        .checked_mul(height as usize)
        .ok_or_else(|| bad("bitmap size overflow"))?;
    if data.len().saturating_sub(data_offset) < needed {
        return Err(bad("truncated pixel data"));
    }
    if image_size != 0 && u64::from(image_size) < needed as u64 {
        return Err(bad(
            "declared image size is smaller than required pixel data",
        ));
    }
    let palette = &data[header_len..data_offset];
    let source = &data[data_offset..data_offset + needed];
    let rgba_len = usize::try_from(pixels)
        .ok()
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| bad("RGBA allocation overflow"))?;
    let mut rgba = vec![0; rgba_len];
    for y in 0..height as usize {
        let source_y = if signed_height > 0 {
            height as usize - 1 - y
        } else {
            y
        };
        let row = &source[source_y * stride..(source_y + 1) * stride];
        for x in 0..width as usize {
            let (red, green, blue) = match bpp {
                32 => (row[x * 4 + 2], row[x * 4 + 1], row[x * 4]),
                24 => (row[x * 3 + 2], row[x * 3 + 1], row[x * 3]),
                8 => palette_color(palette, usize::from(row[x]), record_index)?,
                4 => {
                    let value = row[x / 2];
                    let index = if x % 2 == 0 { value >> 4 } else { value & 15 };
                    palette_color(palette, usize::from(index), record_index)?
                }
                1 => {
                    let index = (row[x / 8] >> (7 - x % 8)) & 1;
                    palette_color(palette, usize::from(index), record_index)?
                }
                _ => unreachable!(),
            };
            let output = (y * width as usize + x) * 4;
            rgba[output..output + 4].copy_from_slice(&[red, green, blue, 255]);
        }
    }
    Ok(DecodedDib {
        bitmap: Bitmap {
            width,
            height,
            rgba,
        },
        top_down: signed_height < 0,
    })
}

pub fn crop_bitmap(
    bitmap: Bitmap,
    top_down: bool,
    source_x: i32,
    source_y: i32,
    source_width: i32,
    source_height: i32,
    record_index: usize,
) -> Result<Bitmap> {
    let bad = |message: &str| MetafileError::InvalidBitmap {
        record_index,
        message: message.into(),
    };
    if source_width == 0 || source_height == 0 {
        return Err(bad("source crop has a zero extent"));
    }
    let interval = |origin: i32, extent: i32, maximum: u32| -> Result<(u32, u32, bool)> {
        let end = origin
            .checked_add(extent)
            .ok_or_else(|| bad("source crop overflow"))?;
        let start = origin.min(end);
        let finish = origin.max(end);
        if start < 0 || i64::from(finish) > i64::from(maximum) {
            return Err(bad("source crop lies outside bitmap"));
        }
        Ok((
            u32::try_from(start).map_err(|_| bad("source crop overflow"))?,
            u32::try_from(finish).map_err(|_| bad("source crop overflow"))?,
            extent < 0,
        ))
    };
    let (left, right, flip_x) = interval(source_x, source_width, bitmap.width)?;
    let (native_top, native_bottom, flip_y) = interval(source_y, source_height, bitmap.height)?;
    let (top, bottom) = if top_down {
        (native_top, native_bottom)
    } else {
        (bitmap.height - native_bottom, bitmap.height - native_top)
    };
    let width = right - left;
    let height = bottom - top;
    if left == 0
        && top == 0
        && width == bitmap.width
        && height == bitmap.height
        && !flip_x
        && !flip_y
    {
        return Ok(bitmap);
    }
    let capacity = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| bad("source crop allocation overflow"))?;
    let mut rgba = Vec::with_capacity(
        usize::try_from(capacity).map_err(|_| bad("source crop allocation overflow"))?,
    );
    for output_y in 0..height {
        let row = if flip_y {
            bottom - 1 - output_y
        } else {
            top + output_y
        };
        for output_x in 0..width {
            let column = if flip_x {
                right - 1 - output_x
            } else {
                left + output_x
            };
            let pixel = (u64::from(row) * u64::from(bitmap.width) + u64::from(column)) * 4;
            let pixel = usize::try_from(pixel).map_err(|_| bad("source pixel offset overflow"))?;
            rgba.extend_from_slice(&bitmap.rgba[pixel..pixel + 4]);
        }
    }
    Ok(Bitmap {
        width,
        height,
        rgba,
    })
}

fn palette_color(palette: &[u8], index: usize, record_index: usize) -> Result<(u8, u8, u8)> {
    let offset = index
        .checked_mul(4)
        .ok_or_else(|| MetafileError::InvalidBitmap {
            record_index,
            message: "palette index overflow".into(),
        })?;
    if offset + 3 >= palette.len() {
        return Err(MetafileError::InvalidBitmap {
            record_index,
            message: "palette index out of bounds".into(),
        });
    }
    Ok((palette[offset + 2], palette[offset + 1], palette[offset]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_crops_bottom_up_24_bit() {
        let mut dib = vec![0; 48];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&2i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        dib[40..46].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
        let decoded = decode_dib(&dib, 0, &ResourceLimits::default()).unwrap();
        let cropped = crop_bitmap(decoded.bitmap, decoded.top_down, 1, 0, 1, 1, 0).unwrap();
        assert_eq!(cropped.rgba, [0, 255, 0, 255]);
    }

    #[test]
    fn rejects_truncated_headers() {
        assert!(decode_dib(&[40, 0, 0, 0], 2, &ResourceLimits::default()).is_err());
    }

    #[test]
    fn validates_planes_palette_and_image_size() {
        let base = || {
            let mut dib = vec![0; 44];
            dib[0..4].copy_from_slice(&40u32.to_le_bytes());
            dib[4..8].copy_from_slice(&1i32.to_le_bytes());
            dib[8..12].copy_from_slice(&1i32.to_le_bytes());
            dib[12..14].copy_from_slice(&1u16.to_le_bytes());
            dib[14..16].copy_from_slice(&32u16.to_le_bytes());
            dib
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
    }

    #[test]
    fn bi_rgb_32_bit_reserved_alpha_is_opaque() {
        let mut dib = vec![0; 44];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        dib[40..44].copy_from_slice(&[3, 2, 1, 0]);
        assert_eq!(
            decode_dib(&dib, 0, &ResourceLimits::default())
                .unwrap()
                .bitmap
                .rgba,
            [1, 2, 3, 255]
        );
    }
}
