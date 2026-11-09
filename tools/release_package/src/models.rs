use crate::{
    digest::verified,
    source::{command, source},
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
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
pub(crate) fn model_files(sha: &str, cache: &Path) -> Result<Vec<(String, PathBuf)>> {
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
