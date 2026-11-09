use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}
pub(crate) fn command(exe: &str, args: &[&str]) -> Result<Vec<u8>> {
    command_at(exe, args, &root())
}
pub(crate) fn command_at(exe: &str, args: &[&str], directory: &Path) -> Result<Vec<u8>> {
    let result = Command::new(exe)
        .args(args)
        .current_dir(directory)
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "{exe} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result.stdout)
}
pub(crate) fn git(args: &[&str]) -> Result<Vec<u8>> {
    command("git", args)
}
pub(crate) fn api(path: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&command("gh", &["api", path])?)?)
}
pub(crate) fn source(sha: &str, path: &str) -> Result<Vec<u8>> {
    git(&["show", &format!("{sha}:{path}")])
}
#[cfg(feature = "package")]
pub(crate) fn text(bytes: &[u8]) -> Result<String> {
    Ok(String::from_utf8(bytes.to_vec())?.replace("\r\n", "\n"))
}
pub(crate) fn validate_tag(tag: &str) -> Result<()> {
    let v = tag
        .strip_prefix('v')
        .context("Version tag must start with v")?;
    let base = v.split_once('-').map(|p| p.0).unwrap_or(v);
    let numbers = base.split('.').collect::<Vec<_>>();
    anyhow::ensure!(
        numbers.len() == 3
            && numbers
                .iter()
                .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c)),
        "Invalid version tag"
    );
    Ok(())
}
pub(crate) fn validate_sha(sha: &str) -> Result<()> {
    anyhow::ensure!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "Expected full source commit SHA"
    );
    Ok(())
}
pub(crate) fn fetch_tag_commit(directory: &Path, tag: &str) -> Result<String> {
    validate_tag(tag)?;
    // Checkout can synthesize a lightweight local tag for an annotated remote
    // tag. Fetch into FETCH_HEAD so validation never replaces either tag.
    command_at(
        "git",
        &[
            "fetch",
            "--no-tags",
            "--depth=1",
            "origin",
            &format!("refs/tags/{tag}"),
        ],
        directory,
    )?;
    let sha = String::from_utf8(command_at(
        "git",
        &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
        directory,
    )?)?
    .trim()
    .to_owned();
    validate_sha(&sha)?;
    Ok(sha)
}
pub(crate) fn stage(tag: &str, run: &str) -> Result<()> {
    validate_tag(tag)?;
    anyhow::ensure!(
        !run.is_empty() && run.chars().all(|c| c.is_ascii_digit()),
        "Invalid CI run ID"
    );
    let repo = std::env::var("GITHUB_REPOSITORY")?;
    anyhow::ensure!(repo == "P-JSL/Syousetu_Translate", "Unexpected repository");
    let sha = fetch_tag_commit(&root(), tag)?;
    let ci = api(&format!("repos/{repo}/actions/runs/{run}"))?;
    anyhow::ensure!(
        ci["name"] == "Rust CI"
            && ci["status"] == "completed"
            && ci["conclusion"] == "success"
            && ci["head_sha"] == sha,
        "CI must succeed for the exact tagged source"
    );
    let jobs = api(&format!(
        "repos/{repo}/actions/runs/{run}/jobs?per_page=100"
    ))?;
    let jobs = jobs["jobs"].as_array().context("CI jobs missing")?;
    for required in ["checks (windows-latest)", "lint", "legacy"] {
        anyhow::ensure!(
            jobs.iter()
                .any(|j| j["name"] == required && j["conclusion"] == "success"),
            "Required CI job missing: {required}"
        );
    }
    anyhow::ensure!(
        jobs.iter().any(|j| (j["name"] == "checks (ubuntu-24.04)"
            || j["name"] == "checks (ubuntu-latest)")
            && j["conclusion"] == "success"),
        "Required Linux CI job missing"
    );
    let releases = api(&format!("repos/{repo}/releases?per_page=100"))?;
    let releases = releases
        .as_array()
        .context("Release list missing")?
        .iter()
        .filter(|r| r["tag_name"] == tag)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        releases.len() == 1 && releases[0]["draft"] == true,
        "Only an existing unique release draft may receive assets"
    );
    let assets = api(&format!(
        "repos/{repo}/actions/runs/{run}/artifacts?per_page=100"
    ))?;
    let matches = assets["artifacts"]
        .as_array()
        .context("Artifacts missing")?
        .iter()
        .filter(|a| a["name"] == "SyousetuTranslate-Web-Windows-x64" && a["expired"] == false)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        matches.len() == 1 && matches[0]["workflow_run"]["head_sha"] == sha,
        "Expected exactly one source-matched Windows artifact"
    );
    fs::write(
        root().join("release-artifact.json"),
        serde_json::to_vec(matches[0])?,
    )?;
    let mut output = OpenOptions::new()
        .append(true)
        .open(std::env::var("GITHUB_OUTPUT")?)?;
    writeln!(
        output,
        "source_sha={sha}\nartifact_name=SyousetuTranslate-Web-Windows-x64"
    )?;
    Ok(())
}
