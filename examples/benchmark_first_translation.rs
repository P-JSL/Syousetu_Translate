//! Compare complete first translations in fresh processes; never use result reuse.
//! All generated novel text and process logs remain in a caller-selected local folder.
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
    control: PathBuf,
    candidate: PathBuf,
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
    trial: usize,
    variant: &'static str,
    seconds: f64,
    sha256: String,
    verification: serde_json::Value,
}
fn run(input: &Inputs, trial: usize, variant: &'static str, cli: &Path) -> Result<Sample> {
    let name = format!("{trial}-{variant}");
    let target = input.output.join(format!("{name}.txt"));
    let start = Instant::now();
    let result = Command::new(cli)
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
        .arg(&target)
        .arg("--model-dir")
        .arg(&input.model)
        .args([
            "--no-learning",
            "--no-result-cache",
            "--parallel-mode",
            "parallel",
            "--jobs",
            "128",
            "--japanese",
            "--no-delay",
        ])
        .env("CANDLE_NUM_THREADS", "1")
        .output()?;
    let seconds = start.elapsed().as_secs_f64();
    fs::write(input.output.join(format!("{name}.log")), &result.stderr)?;
    ensure!(
        result.status.success(),
        "{name}: failed, see private process log"
    );
    ensure!(
        !String::from_utf8_lossy(&result.stderr).contains("번역 결과 재사용 ·"),
        "{name}: result reuse unexpectedly enabled"
    );
    let verified = Command::new(&input.verifier)
        .arg(&input.episodes)
        .arg(&target)
        .arg(input.chapters.to_string())
        .output()?;
    ensure!(
        verified.status.success(),
        "{name}: structural verification failed"
    );
    Ok(Sample {
        trial,
        variant,
        seconds,
        sha256: sha256_file(&target)?,
        verification: serde_json::from_slice(&verified.stdout)?,
    })
}
fn median(samples: &[Sample], variant: &str) -> f64 {
    let mut times = samples
        .iter()
        .filter(|sample| sample.variant == variant)
        .map(|sample| sample.seconds)
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 9,
        "usage: benchmark_first_translation CONTROL_CLI CANDIDATE_CLI MODEL URL INDEX_HTML EPISODES CHAPTERS ORIGINAL_SECONDS NEW_OUTPUT_DIR");
    let output = std::env::current_dir()?.join(&args[8]);
    ensure!(
        !output.exists(),
        "use a new output folder to preserve earlier measurements"
    );
    let verifier = std::env::current_exe()?
        .parent()
        .context("benchmark location")?
        .join(format!(
            "verify_saved_novel{}",
            std::env::consts::EXE_SUFFIX
        ));
    ensure!(
        verifier.is_file(),
        "build verify_saved_novel beside this example"
    );
    let input = Inputs {
        control: fs::canonicalize(&args[0])?,
        candidate: fs::canonicalize(&args[1])?,
        model: fs::canonicalize(&args[2])?,
        url: args[3].clone(),
        index: fs::canonicalize(&args[4])?,
        episodes: fs::canonicalize(&args[5])?,
        chapters: args[6].parse()?,
        original_seconds: args[7].parse()?,
        output,
        verifier,
    };
    ensure!(
        input.chapters > 0 && input.original_seconds.is_finite() && input.original_seconds > 0.,
        "invalid corpus size/reference"
    );
    let hashes = [sha256_file(&input.control)?, sha256_file(&input.candidate)?];
    let model_path = input.model.join("ja-ko/model-int8.gguf");
    let model_hash = sha256_file(&model_path)?;
    fs::create_dir_all(&input.output)?;
    let mut samples: Vec<Sample> = Vec::new();
    for trial in 1..=3 {
        let mut variants = [("control", &input.control), ("candidate", &input.candidate)];
        if trial % 2 == 0 {
            variants.reverse();
        }
        for (variant, cli) in variants {
            let sample = run(&input, trial, variant, cli)?;
            if let Some(reference) = samples.first() {
                ensure!(
                    sample.sha256 == reference.sha256,
                    "{trial}/{variant}: output changed; no quality-preserving speed claim"
                );
            }
            samples.push(sample);
            fs::write(
                input.output.join("samples.json"),
                serde_json::to_vec_pretty(&samples)?,
            )?;
        }
    }
    ensure!(
        hashes == [sha256_file(&input.control)?, sha256_file(&input.candidate)?]
            && model_hash == sha256_file(&model_path)?,
        "benchmark executable/model changed"
    );
    let control_seconds = median(&samples, "control");
    let candidate_seconds = median(&samples, "candidate");
    let report = serde_json::json!({
        "url":input.url,"chapters":input.chapters,"control_sha256":hashes[0],
        "candidate_sha256":hashes[1],"model_sha256":model_hash,
        "logical_contexts":128,"parallel_policy":"parallel","new_process_per_sample":true,
        "correction_memory":false,"disk_result_reuse":false,"cached_source_html":true,
        "os_file_cache_cleared":false,"alternating_process_order":true,"samples":samples,
        "control_median_seconds":control_seconds,"candidate_median_seconds":candidate_seconds,
        "candidate_time_reduction_percent":100.*(1.-candidate_seconds/control_seconds),
        "original_v032_seconds":input.original_seconds,
        "time_reduction_from_v032_percent":100.*(1.-candidate_seconds/input.original_seconds),
        "first_translation_90_percent_met":candidate_seconds <= input.original_seconds*0.1,
        "first_translation_95_percent_met":candidate_seconds <= input.original_seconds*0.05,
        "all_full_work_structural_checks_passed":true,"all_outputs_byte_identical":true,
        "semantic_accuracy_certified":false
    });
    fs::write(
        input.output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
