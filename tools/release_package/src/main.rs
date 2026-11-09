//! Rust-only, streaming portable release packaging. No ML/GUI crate dependency.
use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
#[cfg(feature = "package")]
use std::{collections::BTreeMap, time::Duration};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};
#[allow(dead_code)]
#[path = "../../release_audit.rs"]
mod audit;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}
fn command(exe: &str, args: &[&str]) -> Result<Vec<u8>> {
    command_at(exe, args, &root())
}
fn command_at(exe: &str, args: &[&str], directory: &Path) -> Result<Vec<u8>> {
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
fn git(args: &[&str]) -> Result<Vec<u8>> {
    command("git", args)
}
fn api(path: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&command("gh", &["api", path])?)?)
}
fn source(sha: &str, path: &str) -> Result<Vec<u8>> {
    git(&["show", &format!("{sha}:{path}")])
}
#[cfg(feature = "package")]
fn text(bytes: &[u8]) -> Result<String> {
    Ok(String::from_utf8(bytes.to_vec())?.replace("\r\n", "\n"))
}
fn validate_tag(tag: &str) -> Result<()> {
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
fn validate_sha(sha: &str) -> Result<()> {
    anyhow::ensure!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "Expected full source commit SHA"
    );
    Ok(())
}
fn fetch_tag_commit(directory: &Path, tag: &str) -> Result<String> {
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
fn stage(tag: &str, run: &str) -> Result<()> {
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
fn hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn verified(path: &Path, size: u64, digest: &str) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == size)
        && hash(path).is_ok_and(|h| h == digest)
}
#[cfg(feature = "package")]
fn client() -> Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .user_agent("SyousetuTranslate/Rust-release-package");
    if let Some(path) = ["SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE"]
        .iter()
        .find_map(|k| std::env::var_os(k).filter(|s| !s.is_empty()))
    {
        for cert in reqwest::Certificate::from_pem_bundle(&fs::read(path)?)? {
            builder = builder.add_root_certificate(cert);
        }
    }
    Ok(builder.build()?)
}
#[cfg(feature = "package")]
fn model_files(sha: &str, cache: &Path) -> Result<Vec<(String, PathBuf)>> {
    let manifest: Value = serde_json::from_slice(&source(sha, "models/downloads.json")?)?;
    let assets = manifest
        .as_array()
        .context("Model download manifest missing")?;
    let mut result = Vec::new();
    let mut found = BTreeSet::new();
    let http = client()?;
    for a in assets {
        let name = a["file"].as_str().context("Model file missing")?;
        anyhow::ensure!(
            a["model"] == "ja-ko"
                && [
                    "config.json",
                    "model-int8.gguf",
                    "sentencepiece.bpe.model",
                    "vocab.json"
                ]
                .contains(&name)
                && found.insert(name.to_owned()),
            "Unexpected model file"
        );
        let size = a["size"].as_u64().context("Model size missing")?;
        let digest = a["sha256"].as_str().context("Model hash missing")?;
        let path = cache.join("ja-ko").join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        if !verified(&path, size, digest) {
            let temporary = path.with_extension("part");
            if name == "model-int8.gguf" {
                let release = a["github_release_api"]
                    .as_str()
                    .context("Model release missing")?
                    .rsplit('/')
                    .next()
                    .context("Model tag missing")?;
                anyhow::ensure!(
                    release == "models-small100-int8-v1",
                    "Unexpected model release"
                );
                // GITHUB_TOKEN stays inside the normal gh credential flow.
                command(
                    "gh",
                    &[
                        "release",
                        "download",
                        release,
                        "--repo",
                        "P-JSL/Syousetu_Translate",
                        "--pattern",
                        name,
                        "--dir",
                        cache.join("ja-ko").to_str().context("Invalid cache path")?,
                        "--clobber",
                    ],
                )?;
            } else {
                let mut response = http
                    .get(a["url"].as_str().context("Model URL missing")?)
                    .send()?
                    .error_for_status()?;
                let mut file = File::create(&temporary)?;
                let count = std::io::copy(&mut response.by_ref().take(size + 1), &mut file)?;
                file.sync_all()?;
                drop(file);
                anyhow::ensure!(
                    count == size && verified(&temporary, size, digest),
                    "Downloaded model integrity failed: {name}"
                );
                if path.exists() {
                    fs::remove_file(&path)?;
                }
                fs::rename(temporary, &path)?;
            }
        }
        anyhow::ensure!(
            verified(&path, size, digest),
            "Model integrity failed: {name}"
        );
        result.push((format!("models/ja-ko/{name}"), path));
    }
    anyhow::ensure!(found.len() == 4, "Incomplete bundled model");
    Ok(result)
}
#[cfg(any(feature = "package", test))]
fn timestamp(seconds: i64) -> Result<zip::DateTime> {
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
fn package(
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
fn verify_portable(
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
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("stage-web") if args.len()==3 => stage(&args[1],&args[2]),
        #[cfg(feature="package")]
        Some("package-web") if args.len()==8 => package(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]),&args[4],&args[5],args[6].parse()?,Path::new(&args[7])),
        Some("verify-portable") if args.len()==6 => verify_portable(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4]),&args[5]),
        _ => anyhow::bail!("Usage: stage-web TAG RUN | package-web ARTIFACT METADATA OUT TAG COMMIT RUN CACHE | verify-portable ZIP SUMS OUT EXPECTED_CLIENT COMMIT"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
