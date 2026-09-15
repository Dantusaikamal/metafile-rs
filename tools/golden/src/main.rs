use std::{env, error::Error, fs, fs::File, io::BufReader, path::Path};

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

fn content_bounds(image: &Image) -> Option<(u32, u32, u32, u32)> {
    let background = &image.rgba[..4];
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height {
        for x in 0..image.width {
            let offset = ((y * image.width + x) * 4) as usize;
            if &image.rgba[offset..offset + 4] != background {
                bounds = Some(bounds.map_or((x, y, x + 1, y + 1), |(l, t, r, b)| {
                    (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1))
                }));
            }
        }
    }
    bounds
}

struct Thresholds {
    mae: f64,
    rms: f64,
    differing: f64,
    bounds_delta: u32,
}

fn profile(name: &str) -> Result<Thresholds, Box<dyn Error>> {
    Ok(match name {
        "vector" => Thresholds {
            mae: 8.0,
            rms: 30.0,
            differing: 15.0,
            bounds_delta: 3,
        },
        "bitmap" => Thresholds {
            mae: 15.0,
            rms: 38.0,
            // Windows GDI+ and resvg use different interpolation kernels. Keep
            // geometry tightly gated while allowing the measured resampling
            // noise across otherwise matching bitmap areas.
            differing: 27.0,
            bounds_delta: 3,
        },
        "gradient" => Thresholds {
            // GDI+ and SVG interpolate gradient channels with measurably
            // different rounding across most painted pixels. Gate aggregate
            // color error and geometry tightly instead of treating every
            // one-channel rounding difference as a structural failure.
            mae: 4.0,
            rms: 12.0,
            differing: 35.0,
            bounds_delta: 3,
        },
        "text" => Thresholds {
            mae: 15.0,
            rms: 50.0,
            differing: 35.0,
            bounds_delta: 8,
        },
        _ => {
            return Err(
                format!("unknown profile {name:?}; expected vector, bitmap, or text").into(),
            )
        }
    })
}

fn bounds_distance(a: Option<(u32, u32, u32, u32)>, b: Option<(u32, u32, u32, u32)>) -> u32 {
    match (a, b) {
        (None, None) => 0,
        (Some(a), Some(b)) => [
            a.0.abs_diff(b.0),
            a.1.abs_diff(b.1),
            a.2.abs_diff(b.2),
            a.3.abs_diff(b.3),
        ]
        .into_iter()
        .max()
        .unwrap_or(0),
        _ => u32::MAX,
    }
}

fn write_difference(
    path: &Path,
    reference: &Image,
    candidate: &Image,
) -> Result<(), Box<dyn Error>> {
    let mut data = Vec::with_capacity(reference.rgba.len());
    for (a, b) in reference.rgba.chunks(4).zip(candidate.rgba.chunks(4)) {
        let delta = (0..3)
            .map(|channel| a[channel].abs_diff(b[channel]))
            .max()
            .unwrap_or(0);
        data.extend_from_slice(&[delta, 0, 0, 255]);
    }
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(file, reference.width, reference.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&data)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let mut profile_name = None;
    let mut metrics_path = None;
    let mut difference_path = None;
    let mut positional = Vec::new();
    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--profile" => profile_name = Some(args.next().ok_or("--profile requires a value")?),
            "--metrics" => metrics_path = Some(args.next().ok_or("--metrics requires a path")?),
            "--difference" => {
                difference_path = Some(args.next().ok_or("--difference requires a path")?)
            }
            _ => positional.push(argument),
        }
    }
    if positional.len() != 2 {
        return Err("usage: metafile-golden [--profile vector|bitmap|gradient|text] [--metrics metrics.json] [--difference difference.png] reference.png candidate.png".into());
    }
    let reference = decode(Path::new(&positional[0]))?;
    let candidate = decode(Path::new(&positional[1]))?;
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
    let reference_alpha_bounds = alpha_bounds(&reference);
    let candidate_alpha_bounds = alpha_bounds(&candidate);
    let reference_bounds = content_bounds(&reference);
    let candidate_bounds = content_bounds(&candidate);
    let mae = absolute as f64 / channels;
    let rms = (squared / channels).sqrt();
    let differing = differing_pixels as f64 * 100.0 / pixels as f64;
    let bounds_delta = bounds_distance(reference_bounds, candidate_bounds);
    let json_bounds = |bounds: Option<(u32, u32, u32, u32)>| {
        bounds.map_or_else(
            || "null".to_owned(),
            |(left, top, right, bottom)| format!("[{left}, {top}, {right}, {bottom}]"),
        )
    };
    let mut passed = true;
    if let Some(name) = profile_name.as_ref() {
        let thresholds = profile(&name.to_string_lossy())?;
        passed = mae <= thresholds.mae
            && rms <= thresholds.rms
            && differing <= thresholds.differing
            && bounds_delta <= thresholds.bounds_delta;
    }
    let json = format!(
        "{{\n  \"dimensions\": [ {}, {} ],\n  \"referenceAlphaBounds\": {},\n  \"candidateAlphaBounds\": {},\n  \"referenceContentBounds\": {},\n  \"candidateContentBounds\": {},\n  \"boundsMaximumDelta\": {},\n  \"meanAbsoluteError\": {:.6},\n  \"rmsError\": {:.6},\n  \"differingPixelPercentage\": {:.6},\n  \"passed\": {}\n}}\n",
        reference.width, reference.height, json_bounds(reference_alpha_bounds), json_bounds(candidate_alpha_bounds), json_bounds(reference_bounds), json_bounds(candidate_bounds), bounds_delta, mae, rms, differing, passed
    );
    print!("{json}");
    if let Some(path) = metrics_path {
        fs::write(path, &json)?;
    }
    if let Some(path) = difference_path {
        write_difference(Path::new(&path), &reference, &candidate)?;
    }
    if !passed {
        return Err("golden comparison exceeded the selected profile thresholds".into());
    }
    Ok(())
}
