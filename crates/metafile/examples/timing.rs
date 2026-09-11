use std::{env, fs, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: cargo run -p metafile-wmf --example timing -- file.wmf")?;
    let bytes = fs::read(path)?;
    let start = Instant::now();
    let out = metafile::to_svg(&bytes, Default::default())?;
    println!(
        "{} bytes -> {} SVG bytes in {:?}",
        bytes.len(),
        out.svg.len(),
        start.elapsed()
    );
    Ok(())
}
