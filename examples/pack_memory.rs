//! Rebuild the embedded correction data entirely in Rust.
use anyhow::Result;
fn main() -> Result<()> {
    let source = std::fs::read("data/learning/ja-ko-natural.json")?;
    let _: syousetu_translate::learning::Memory = serde_json::from_slice(&source)?;
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&source, 9);
    std::fs::write("data/learning/ja-ko-natural.json.zlib", &compressed)?;
    println!(
        "Default corrections: {} -> {} bytes",
        source.len(),
        compressed.len()
    );
    Ok(())
}
