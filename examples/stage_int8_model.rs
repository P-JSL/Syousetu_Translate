//! Maintainer-only Rust model exporter. Never shipped in the app ZIP.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use syousetu_translate::{int8, models};

fn gh(path: &str) -> Result<Value> {
    let result = Command::new("gh").args(["api", path]).output()?;
    anyhow::ensure!(result.status.success(), "GitHub API request failed: {path}");
    Ok(serde_json::from_slice(&result.stdout)?)
}
fn validate_release(tag: &str, run: &str) -> Result<()> {
    anyhow::ensure!(
        tag == "models-small100-int8-v1",
        "Unexpected model release tag"
    );
    anyhow::ensure!(
        run.chars().all(|c| c.is_ascii_digit()) && !run.is_empty(),
        "Invalid CI run ID"
    );
    let repo = std::env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY required")?;
    anyhow::ensure!(repo == "P-JSL/Syousetu_Translate", "Unexpected repository");
    let head = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    let head = String::from_utf8(head.stdout)?.trim().to_owned();
    let ci = gh(&format!("repos/{repo}/actions/runs/{run}"))?;
    anyhow::ensure!(
        ci["name"] == "Rust CI"
            && ci["conclusion"] == "success"
            && ci["status"] == "completed"
            && ci["head_sha"] == head,
        "Successful CI must match model-export source"
    );
    let jobs = gh(&format!(
        "repos/{repo}/actions/runs/{run}/jobs?per_page=100"
    ))?;
    for required in [
        "checks (ubuntu-latest)",
        "checks (windows-latest)",
        "lint",
        "legacy",
    ] {
        anyhow::ensure!(
            jobs["jobs"]
                .as_array()
                .context("CI jobs missing")?
                .iter()
                .any(|j| j["name"] == required && j["conclusion"] == "success"),
            "Required CI job missing: {required}"
        );
    }
    let reference = gh(&format!("repos/{repo}/git/ref/tags/{tag}"))?;
    let commit = if reference["object"]["type"] == "tag" {
        gh(&format!(
            "repos/{repo}/git/tags/{}",
            reference["object"]["sha"]
                .as_str()
                .context("tag SHA missing")?
        ))?["object"]["sha"]
            .clone()
    } else {
        reference["object"]["sha"].clone()
    };
    anyhow::ensure!(commit == head, "Model tag must match validated CI source");
    let releases = gh(&format!("repos/{repo}/releases?per_page=100"))?;
    let matching = releases
        .as_array()
        .context("releases missing")?
        .iter()
        .filter(|r| r["tag_name"] == tag)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        matching.len() == 1 && matching[0]["draft"] == true,
        "Only a unique unpublished model draft may receive files"
    );
    Ok(())
}
fn export(source: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    models::prepare_sources(source, |done, total, label| {
        if done == total {
            eprintln!("source {done}/{total}: {label}");
        }
    })?;
    let original = source.join("ja-ko");
    let spec = int8::convert(
        &original.join("model.safetensors"),
        &out.join("model-int8.gguf"),
    )?;
    anyhow::ensure!(
        spec.sha256 == int8::spec().sha256 && spec.size == int8::spec().size,
        "Non-reproducible INT8 model"
    );
    fs::write(
        out.join("LICENSE-MIT-SMaLL100.txt"),
        include_str!("../licenses/MIT-SMaLL100.txt"),
    )?;
    fs::write(
        out.join("MODEL_CARD.md"),
        include_str!("../models/MODEL_CARD.md"),
    )?;
    let names = [
        "LICENSE-MIT-SMaLL100.txt",
        "MODEL_CARD.md",
        "model-int8.gguf",
    ];
    let sums = names
        .iter()
        .map(|name| Ok(format!("{}  {name}\n", int8::hash(&out.join(name))?)))
        .collect::<Result<String>>()?;
    fs::write(out.join("SHA256SUMS.txt"), sums)?;
    println!("{}", serde_json::to_string_pretty(&spec)?);
    Ok(())
}
fn main() -> Result<()> {
    std::env::set_var("CANDLE_NUM_THREADS", "1");
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let source = PathBuf::from(args.first().context("source directory required")?);
    let out = PathBuf::from(args.get(1).context("output directory required")?);
    if let Some(tag) = args.get(2) {
        validate_release(tag, args.get(3).context("CI run ID required")?)?;
    }
    export(&source, &out)
}
