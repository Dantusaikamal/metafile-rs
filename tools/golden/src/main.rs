use std::{env, error::Error, fs::File, io::BufReader, path::Path};

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode(path: &Path) -> Result<Image, Box<dyn Error>> {
    let decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes)?;
    let source = &bytes[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => source.to_vec(),
        png::ColorType::Rgb => source
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err("golden comparator currently requires RGB or RGBA PNGs".into()),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn alpha_bounds(image: &Image) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height {
        for x in 0..image.width {
            if image.rgba[((y * image.width + x) * 4 + 3) as usize] != 0 {
                bounds = Some(bounds.map_or((x, y, x + 1, y + 1), |(l, t, r, b)| {
                    (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1))
                }));
            }
        }
    }
    bounds
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let reference = decode(Path::new(
        &args
            .next()
            .ok_or("usage: metafile-golden reference.png candidate.png")?,
    ))?;
    let candidate = decode(Path::new(&args.next().ok_or("missing candidate PNG")?))?;
    if (reference.width, reference.height) != (candidate.width, candidate.height) {
        return Err(format!(
            "dimension mismatch: {}x{} versus {}x{}",
            reference.width, reference.height, candidate.width, candidate.height
        )
        .into());
    }
    let mut squared = 0f64;
    let mut absolute = 0u64;
    let mut differing_pixels = 0u64;
    for (a, b) in reference.rgba.chunks(4).zip(candidate.rgba.chunks(4)) {
        let mut differs = false;
        for channel in 0..4 {
            let delta = i16::from(a[channel]) - i16::from(b[channel]);
            differs |= delta != 0;
            absolute += u64::from(delta.unsigned_abs());
            squared += f64::from(delta).powi(2);
        }
        differing_pixels += u64::from(differs);
    }
    let channels = reference.rgba.len() as f64;
    let pixels = u64::from(reference.width) * u64::from(reference.height);
    println!("dimensions={}x{}", reference.width, reference.height);
    println!("reference_alpha_bounds={:?}", alpha_bounds(&reference));
    println!("candidate_alpha_bounds={:?}", alpha_bounds(&candidate));
    println!("mean_absolute_error={:.6}", absolute as f64 / channels);
    println!("rms_error={:.6}", (squared / channels).sqrt());
    println!(
        "differing_pixel_percentage={:.6}",
        differing_pixels as f64 * 100.0 / pixels as f64
    );
    Ok(())
}
