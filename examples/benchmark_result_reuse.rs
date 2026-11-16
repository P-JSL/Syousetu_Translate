//! Compare fresh result stores and reused results in separate real CLI processes.
//! Uses cached HTML and the independent full-novel verifier; never prints novel text.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use syousetu_translate::storage::sha256_file;

struct Inputs {
    cli: PathBuf,
    model: PathBuf,
    url: String,
    index: PathBuf,
    episodes: PathBuf,
    chapters: usize,
    original_seconds: f64,
    output: PathBuf,
    verifier: PathBuf,
}
#[derive(Serialize)]
struct Sample {
    kind: String,
    seconds: f64,
    sha256: String,
    verification: serde_json::Value,
    result_reuse_status: Option<String>,
}
fn run(input: &Inputs, kind: &str, cache: Option<&Path>) -> Result<Sample> {
    let output = input.output.join(format!("{kind}.txt"));
    let mut command = Command::new(&input.cli);
    command
        .args(["download", "--url", &input.url, "--html"])
        .arg(&input.index)
        .arg("--episode-html-dir")
        .arg(&input.episodes)
        .args([
            "--start",
            "1",
            "--end",
            &input.chapters.to_string(),
            "--out",
        ])
        .arg(&output)
        .arg("--model-dir")
        .arg(&input.model)
        .args([
            "--no-learning",
            "--parallel-mode",
            "parallel",
            "--jobs",
            "128",
            "--japanese",
            "--no-delay",
        ])
        .env("CANDLE_NUM_THREADS", "1");
    if let Some(directory) = cache {
        command
            .arg("--reuse-results")
            .arg("--result-cache-dir")
            .arg(directory);
    } else {
        command.arg("--no-result-cache");
    }
    let start = Instant::now();
    let result = command.output()?;
    let seconds = start.elapsed().as_secs_f64();
    fs::write(input.output.join(format!("{kind}.log")), &result.stderr)?;
    ensure!(
        result.status.success(),
        "{kind}: translation failed, see its private log"
    );
    let verified = Command::new(&input.verifier)
        .arg(&input.episodes)
        .arg(&output)
        .arg(input.chapters.to_string())
        .output()?;
    ensure!(
        verified.status.success(),
        "{kind}: structural verification failed: {}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let status = String::from_utf8_lossy(&result.stderr)
        .lines()
        .find(|line| line.starts_with("번역 결과 재사용 ·"))
        .map(str::to_owned);
    ensure!(
        cache.is_none() || status.is_some(),
        "{kind}: result reuse was not initialized"
    );
    Ok(Sample {
        kind: kind.to_owned(),
        seconds,
        sha256: sha256_file(&output)?,
        verification: serde_json::from_slice(&verified.stdout)?,
        result_reuse_status: status,
    })
}
fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 8, "usage: benchmark_result_reuse CLI MODEL URL INDEX_HTML EPISODE_DIR CHAPTERS ORIGINAL_SECONDS NEW_OUTPUT_DIR");
    let output = std::env::current_dir()?.join(&args[7]);
    ensure!(
        !output.exists(),
        "use a new output directory to preserve earlier measurements"
    );
    fs::create_dir_all(&output)?;
    let verifier = std::env::current_exe()?
        .parent()
        .context("benchmark location")?
        .join(format!(
            "verify_saved_novel{}",
            std::env::consts::EXE_SUFFIX
        ));
    ensure!(
        verifier.is_file(),
        "build the verify_saved_novel example beside this benchmark"
    );
    let input = Inputs {
        cli: fs::canonicalize(&args[0])?,
        model: fs::canonicalize(&args[1])?,
        url: args[2].clone(),
        index: fs::canonicalize(&args[3])?,
        episodes: fs::canonicalize(&args[4])?,
        chapters: args[5].parse()?,
        original_seconds: args[6].parse()?,
        output,
        verifier,
    };
    ensure!(
        input.chapters > 0 && input.original_seconds.is_finite() && input.original_seconds > 0.0,
        "invalid corpus size/reference"
    );
    let binary_hash = sha256_file(&input.cli)?;
    let reference = run(&input, "without-disk-reuse", None)?;
    let mut cold = Vec::new();
    let mut warm = Vec::new();
    for iteration in 1..=3 {
        let cache = input.output.join(format!("result-store-{iteration}"));
        ensure!(!cache.exists(), "cold result store already exists");
        let fresh = run(&input, &format!("fresh-store-{iteration}"), Some(&cache))?;
        let reused = run(&input, &format!("reused-store-{iteration}"), Some(&cache))?;
        ensure!(
            fresh.sha256 == reference.sha256 && reused.sha256 == reference.sha256,
            "translation bytes changed in reuse iteration {iteration}"
        );
        cold.push(fresh);
        warm.push(reused);
    }
    ensure!(
        sha256_file(&input.cli)? == binary_hash,
        "CLI changed during the benchmark"
    );
    let cold_median = median(cold.iter().map(|sample| sample.seconds));
    let warm_median = median(warm.iter().map(|sample| sample.seconds));
    let report = serde_json::json!({
        "url":input.url,"chapters":input.chapters,"cli_sha256":binary_hash,
        "model_sha256":sha256_file(&input.model.join("ja-ko/model-int8.gguf"))?,
        "logical_contexts":128,"parallel_policy":"parallel","correction_memory":false,
        "cached_source_html":true,"os_file_cache_cleared":false,"new_process_per_sample":true,
        "without_disk_reuse":reference,"fresh_result_stores":cold,"reused_result_stores":warm,
        "fresh_median_seconds":cold_median,"reused_median_seconds":warm_median,
        "retranslation_reduction_percent":100.0*(1.0-warm_median/cold_median),
        "original_reference_seconds":input.original_seconds,
        "first_translation_90_percent_met":cold_median <= input.original_seconds*0.1,
        "first_translation_95_percent_met":cold_median <= input.original_seconds*0.05,
        "all_outputs_byte_identical":true,"all_full_work_structural_checks_passed":true,
        "semantic_accuracy_certified":false,"reused_result_claim_applies_to_first_translation":false
    });
    fs::write(
        input.output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
