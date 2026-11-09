use anyhow::{Context, Result};
use std::path::PathBuf;
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let source = PathBuf::from(args.first().context("source model.safetensors required")?);
    let output = PathBuf::from(args.get(1).context("output model-int8.gguf required")?);
    println!(
        "{}",
        serde_json::to_string_pretty(&syousetu_translate::int8::convert(&source, &output)?)?
    );
    Ok(())
}
