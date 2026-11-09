use crate::{
    audit,
    digest::{hash, verified},
    source::{source, validate_sha},
};
#[cfg(feature = "package")]
use crate::{
    models::model_files,
    source::{git, text, validate_tag},
};
use anyhow::{Context, Result};
use serde_json::Value;
#[cfg(feature = "package")]
use std::{collections::BTreeMap, io::Write};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    path::Path,
};
#[cfg(any(feature = "package", test))]
pub(crate) fn timestamp(seconds: i64) -> Result<zip::DateTime> {
    // Gregorian civil date from Unix days (Howard Hinnant's civil_from_days).
    let seconds = seconds.max(315_532_800);
    let days = seconds / 86400;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    zip::DateTime::from_date_and_time(
        y as u16,
        m as u8,
        d as u8,
        ((seconds % 86400) / 3600) as u8,
        ((seconds % 3600) / 60) as u8,
        (seconds % 60) as u8,
    )
    .map_err(|e| anyhow::anyhow!("ZIP timestamp: {e:?}"))
}
#[cfg(feature = "package")]
pub(crate) fn package(
    artifact: &Path,
    metadata: &Path,
    out: &Path,
    tag: &str,
    sha: &str,
    run: u64,
    cache: &Path,
) -> Result<()> {
    validate_tag(tag)?;
    validate_sha(sha)?;
    audit::web(artifact).map_err(anyhow::Error::msg)?;
    let meta: Value = serde_json::from_slice(&fs::read(metadata)?)?;
    anyhow::ensure!(
        meta["name"] == "SyousetuTranslate-Web-Windows-x64"
            && meta["workflow_run"]["id"] == run
            && meta["workflow_run"]["head_sha"] == sha
            && meta["expired"] == false,
        "Artifact provenance mismatch"
    );
    let cargo = String::from_utf8(source(sha, "Cargo.toml")?)?;
    let version = cargo
        .lines()
        .find_map(|line| line.strip_prefix("version = ").map(|s| s.trim_matches('"')))
        .context("Package version missing")?;
    anyhow::ensure!(
        tag.strip_prefix('v') == Some(version),
        "Tag/package version mismatch"
    );
    let mut paths = BTreeMap::new();
    for name in audit::FILES {
        let p = artifact.join(name);
        if !name.ends_with(".exe") {
            let original = if name == "README.md" {
                "docs/windows-web-client.md"
            } else {
                name
            };
            anyhow::ensure!(
                text(&fs::read(&p)?)? == text(&source(sha, original)?)?,
                "Artifact source mismatch: {name}"
            );
        }
        paths.insert(name.to_owned(), p);
    }
    for (name, path) in model_files(sha, cache)? {
        paths.insert(name, path);
    }
    anyhow::ensure!(paths.len() == 12, "Unexpected portable payload count");
    let mut guide = text(&fs::read(&paths["README.md"])?)?.into_bytes();
    let imports = audit::imports(&fs::read(&paths["syousetu-translate.exe"])?, 3)
        .map_err(anyhow::Error::msg)?;
    if imports.iter().any(|s| s == "vcruntime140.dll") {
        guide.extend_from_slice("\n\n## Windows 실행 런타임\n\n이 빌드는 Microsoft Visual C++ v14 x64 런타임을 사용합니다. VCRUNTIME140.dll 오류가 표시되면 공식 x64 런타임을 설치해 주세요.\nhttps://learn.microsoft.com/cpp/windows/latest-supported-vc-redist\n".as_bytes());
    }
    fs::create_dir_all(out)?;
    let name = format!("SyousetuTranslate-Web-{tag}-Windows-x64.zip");
    let archive_path = out.join(&name);
    let date = timestamp(
        String::from_utf8(git(&["show", "-s", "--format=%ct", sha])?)?
            .trim()
            .parse()?,
    )?;
    let mut archive = zip::ZipWriter::new(File::create(&archive_path)?);
    for (name, path) in &paths {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(if name.ends_with(".gguf") { 3 } else { 9 }))
            .last_modified_time(date)
            .unix_permissions(if name.ends_with(".exe") || name.ends_with(".bat") {
                0o755
            } else {
                0o644
            });
        archive.start_file(name, options)?;
        if name == "README.md" {
            archive.write_all(&guide)?;
        } else {
            std::io::copy(&mut File::open(path)?, &mut archive)?;
        }
    }
    archive.finish()?.sync_all()?;
    let mut verify = zip::ZipArchive::new(File::open(&archive_path)?)?;
    anyhow::ensure!(verify.len() == 12, "Portable archive file count mismatch");
    for i in 0..verify.len() {
        let mut entry = verify.by_index(i)?;
        std::io::copy(&mut entry, &mut std::io::sink())?;
    }
    let digest = hash(&archive_path)?;
    fs::write(out.join("SHA256SUMS.txt"), format!("{digest}  {name}\n"))?;
    println!(
        "{name}: {} bytes, SHA-256 {digest}, model included",
        fs::metadata(archive_path)?.len()
    );
    Ok(())
}
pub(crate) fn verify_portable(
    zip_path: &Path,
    sums: &Path,
    out: &Path,
    expected_client: &Path,
    sha: &str,
) -> Result<()> {
    validate_sha(sha)?;
    let filename = zip_path
        .file_name()
        .context("ZIP filename missing")?
        .to_str()
        .context("ZIP filename invalid")?;
    let digest = hash(zip_path)?;
    anyhow::ensure!(
        fs::read_to_string(sums)?
            .split_whitespace()
            .collect::<Vec<_>>()
            == [digest.as_str(), filename],
        "Published ZIP checksum mismatch"
    );
    let mut expected = audit::FILES
        .iter()
        .map(|s| s.to_string())
        .collect::<BTreeSet<_>>();
    for name in [
        "config.json",
        "model-int8.gguf",
        "sentencepiece.bpe.model",
        "vocab.json",
    ] {
        expected.insert(format!("models/ja-ko/{name}"));
    }
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?)?;
    let actual = archive
        .file_names()
        .map(|name| name.map(|s| s.into_owned()))
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    anyhow::ensure!(
        actual == expected && archive.len() == 12,
        "Portable ZIP must contain exactly the 12 allowed files"
    );
    anyhow::ensure!(
        !out.exists(),
        "ZIP verification needs an absent output directory"
    );
    fs::create_dir_all(out)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        anyhow::ensure!(
            !entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000),
            "Symlinks are forbidden"
        );
        let destination = out.join(entry.name()?.as_ref());
        fs::create_dir_all(destination.parent().context("ZIP path parent missing")?)?;
        std::io::copy(&mut entry, &mut File::create(destination)?)?;
    }
    audit::web(out).map_err(anyhow::Error::msg)?;
    for name in ["syousetu-gui.exe", "syousetu-translate.exe"] {
        anyhow::ensure!(
            hash(&out.join(name))? == hash(&expected_client.join(name))?,
            "Portable executable does not match successful CI: {name}"
        );
    }
    let assets: Value = serde_json::from_slice(&source(sha, "models/downloads.json")?)?;
    for a in assets.as_array().context("Model manifest missing")? {
        let file = a["file"].as_str().context("Model filename missing")?;
        let path = out.join("models/ja-ko").join(file);
        anyhow::ensure!(
            verified(
                &path,
                a["size"].as_u64().context("Model size missing")?,
                a["sha256"].as_str().context("Model hash missing")?
            ),
            "Bundled model failed SHA-256: {file}"
        );
    }
    println!("Portable ZIP verified: exact CI executables, four pinned model assets, CRC and SHA-256, {digest}");
    Ok(())
}
