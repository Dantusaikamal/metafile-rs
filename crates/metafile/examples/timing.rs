use std::{env, fs, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: cargo run -p metafile-wmf --example timing -- file.wmf")?;
    let iterations = env::args()
        .nth(2)
        .map_or(Ok(100usize), |value| value.parse())?;
    let bytes = fs::read(path)?;
    let inspect_start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(metafile::inspect(std::hint::black_box(&bytes))?);
    }
    let inspect = inspect_start.elapsed();
    let start = Instant::now();
    let mut out = None;
    for _ in 0..iterations {
        out = Some(metafile::to_svg(
            std::hint::black_box(&bytes),
            Default::default(),
        )?);
    }
    let render = start.elapsed();
    let out = out.ok_or("iterations must be greater than zero")?;
    println!(
        "input_bytes={} records={} svg_bytes={} iterations={} inspect_mean_us={:.3} render_svg_mean_us={:.3}",
        bytes.len(),
        out.metadata.record_count.unwrap_or(0),
        out.svg.len(),
        iterations,
        inspect.as_secs_f64() * 1_000_000.0 / iterations as f64,
        render.as_secs_f64() * 1_000_000.0 / iterations as f64,
    );
    Ok(())
}
