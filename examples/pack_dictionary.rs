//! Regenerate the reviewable core glossary's embedded Deflate asset in Rust.
use anyhow::{Context, Result};
use std::{collections::BTreeMap, fs, path::Path};
fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("data/dictionary/ja-ko-core.tsv"))?;
    let mut entries = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (ja, ko) = line.split_once('\t').context("TSV separator missing")?;
        anyhow::ensure!(
            !ja.is_empty() && !ko.is_empty() && !ko.contains('\t'),
            "invalid row {}",
            index + 1
        );
        anyhow::ensure!(
            ja.chars()
                .any(|c| matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'))
                && ko.chars().any(|c| ('가'..='힣').contains(&c)),
            "language mismatch at row {}",
            index + 1
        );
        anyhow::ensure!(
            entries.insert(ja, ko).is_none(),
            "duplicate source at row {}",
            index + 1
        );
    }
    let bytes = serde_json::to_vec(
        &serde_json::json!({"version":1,"application":"exact","entries":entries}),
    )?;
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 9);
    fs::write(
        root.join("data/dictionary/ja-ko-core.json.zlib"),
        &compressed,
    )?;
    println!(
        "entries={} json_bytes={} compressed_bytes={}",
        entries.len(),
        bytes.len(),
        compressed.len()
    );
    Ok(())
}
