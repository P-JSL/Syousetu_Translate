//! Maintainer-only corpus comparison. Outputs are evidence, never training labels.
use anyhow::{ensure, Context, Result};
use std::{fs, path::Path, process::Command};
use syousetu_translate::storage::sha256_file;

fn run(binary: &str, model: &str, source: &str, output: &Path) -> Result<Vec<String>> {
    let result = Command::new(binary)
        .args([
            "file",
            "--engine",
            "internal",
            "--input",
            source,
            "--model-dir",
            model,
            "--no-learning",
            "--no-japanese",
            "--parallel-mode",
            "parallel",
            "--jobs",
            "32",
            "--out",
        ])
        .arg(output)
        .env("CANDLE_NUM_THREADS", "1")
        .output()?;
    ensure!(
        result.status.success(),
        "corpus translation failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(fs::read_to_string(output)?
        .lines()
        .map(str::to_owned)
        .collect())
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 5,
        "usage: review_natural_text BASELINE_CLI CANDIDATE_CLI MODEL SOURCE_TXT OUTPUT_DIR"
    );
    let sources = fs::read_to_string(&args[3])?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure!(!sources.is_empty(), "empty corpus");
    let directory = Path::new(&args[4]);
    fs::create_dir_all(directory)?;
    let before = run(&args[0], &args[2], &args[3], &directory.join("before.txt"))?;
    let after = run(&args[1], &args[2], &args[3], &directory.join("after.txt"))?;
    ensure!(
        sources.len() == before.len() && sources.len() == after.len(),
        "missing corpus rows"
    );
    ensure!(
        sources
            .iter()
            .zip(&after)
            .all(|(source, target)| source.trim().is_empty() || !target.trim().is_empty()),
        "empty translated row"
    );
    let cases = sources.iter().zip(&before).zip(&after).enumerate().map(|(index, ((source, before), after))| {
        serde_json::json!({"case":index+1,"source":source,"before":before,"after":after,"changed":before!=after})
    }).collect::<Vec<_>>();
    let model = Path::new(&args[2]).join("ja-ko/model-int8.gguf");
    let report = serde_json::json!({
        "source_sha256":sha256_file(Path::new(&args[3]))?,
        "baseline_binary_sha256":sha256_file(Path::new(&args[0]))?,
        "candidate_binary_sha256":sha256_file(Path::new(&args[1]))?,
        "model_sha256":sha256_file(&model).context("hash pinned model")?,
        "cases":cases,"rows":sources.len(),"changed_rows":before.iter().zip(&after).filter(|(a,b)|a!=b).count(),
        "learning":false,"general_quality_certified":false,"generated_outputs_used_as_training":false
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
