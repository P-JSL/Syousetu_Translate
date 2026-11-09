
use crate::{archive::timestamp, source::*};
use anyhow::{Context, Result};
use std::fs;
#[test]
fn tags_and_source_ids_reject_path_or_command_injection() {
    assert!(validate_tag("v0.3.1").is_ok());
    assert!(validate_tag("v0.3.1-beta.1").is_ok());
    for s in ["models-v1", "v1.2", "v1.2.3/evil", "v1.2.3;echo"] {
        assert!(validate_tag(s).is_err());
    }
    assert!(validate_sha("HEAD").is_err());
}
#[test]
fn zip_dates_are_deterministic_and_valid() {
    assert_eq!(timestamp(0).unwrap().year(), 1980);
    assert_eq!(timestamp(1_735_689_600).unwrap().year(), 2025);
}
#[test]
fn annotated_remote_tag_and_checkout_lightweight_tag_do_not_conflict() -> Result<()> {
    let dir = std::env::temp_dir().join(format!(
        "syousetu-tag-fetch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let remote = dir.join("remote");
    let checkout = dir.join("checkout");
    fs::create_dir_all(&remote)?;
    fs::create_dir_all(&checkout)?;
    command_at("git", &["init"], &remote)?;
    command_at(
        "git",
        &[
            "-c",
            "user.name=Release test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "source",
        ],
        &remote,
    )?;
    command_at(
        "git",
        &[
            "-c",
            "user.name=Release test",
            "-c",
            "user.email=test@example.invalid",
            "tag",
            "-a",
            "v0.3.1",
            "-m",
            "annotated",
        ],
        &remote,
    )?;
    let expected = command_at("git", &["rev-parse", "HEAD"], &remote)?;
    command_at("git", &["init"], &checkout)?;
    command_at(
        "git",
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().context("Test path invalid")?,
        ],
        &checkout,
    )?;
    command_at(
        "git",
        &["fetch", "--no-tags", "--depth=1", "origin", "HEAD"],
        &checkout,
    )?;
    command_at("git", &["tag", "v0.3.1", "FETCH_HEAD"], &checkout)?;
    let before = command_at("git", &["rev-parse", "refs/tags/v0.3.1"], &checkout)?;
    assert_eq!(
        fetch_tag_commit(&checkout, "v0.3.1")?,
        String::from_utf8(expected)?.trim()
    );
    assert_eq!(
        command_at("git", &["rev-parse", "refs/tags/v0.3.1"], &checkout)?,
        before
    );
    assert_eq!(
        command_at("git", &["cat-file", "-t", "refs/tags/v0.3.1"], &remote)?,
        b"tag\n"
    );
    fs::remove_dir_all(dir)?;
    Ok(())
}
