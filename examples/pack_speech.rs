//! Pack authored dialogue/colloquial defaults without Python or new crates.
use anyhow::{Context, Result};
use std::{collections::BTreeMap, fs, path::Path};
fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = fs::read_to_string(root.join("data/learning/ja-ko-speech.tsv"))?;
    let text = text.replace("\\t", "\t");
    fs::write(root.join("data/learning/ja-ko-speech.tsv"), &text)?;
    let mut entries = BTreeMap::new();
    for (index, line) in text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
    {
        // Accept escaped tabs in the editable source, emitting real JSON keys.
        let (ja, ko) = line
            .split_once("\\t")
            .or_else(|| line.split_once('\t'))
            .context("missing TSV separator")?;
        anyhow::ensure!(
            !ja.trim().is_empty() && ko.chars().any(|ch| ('가'..='힣').contains(&ch)),
            "invalid row {}",
            index + 1
        );
        anyhow::ensure!(
            entries.insert(ja, ko).is_none(),
            "duplicate source at row {}",
            index + 1
        );
    }
    let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"entries":entries}))?;
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 9);
    fs::write(
        root.join("data/learning/ja-ko-speech.json.zlib"),
        &compressed,
    )?;
    println!(
        "entries={} compressed_bytes={}",
        entries.len(),
        compressed.len()
    );
    Ok(())
}
