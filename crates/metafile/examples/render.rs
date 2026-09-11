use std::{env, fs, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let input = args
        .next()
        .ok_or("usage: render input.wmf output.svg [diagnostics.json]")?;
    let output = args.next().ok_or("missing output SVG path")?;
    let diagnostics = args.next();
    let bytes = fs::read(&input)?;
    let inspect_started = Instant::now();
    let info = metafile::inspect(&bytes)?;
    let inspect_elapsed = inspect_started.elapsed();
    let render_started = Instant::now();
    let result = metafile::to_svg(&bytes, Default::default())?;
    let render_elapsed = render_started.elapsed();
    fs::write(output, result.svg.as_bytes())?;
    if let Some(path) = diagnostics {
        fs::write(path, serde_json::to_vec_pretty(&result.diagnostics)?)?;
    }
    println!(
        "input_bytes={} records={} svg_bytes={} inspect_us={} render_us={} diagnostics={}",
        bytes.len(),
        info.record_count.unwrap_or(0),
        result.svg.len(),
        inspect_elapsed.as_micros(),
        render_elapsed.as_micros(),
        result.diagnostics.len()
    );
    Ok(())
}
