use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = env::args_os().nth(1).ok_or("usage: inspect input.wmf")?;
    let bytes = fs::read(input)?;
    let info = metafile::inspect(&bytes)?;
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}
