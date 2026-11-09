//! Model lifecycle facade: immutable manifests, discovery, verification and transport.
mod download;
mod integrity;
mod manifest;
mod paths;
use crate::storage::sha256_file as hash_file;
use anyhow::Result;
use integrity::verify_file;
pub use integrity::{verify, verify_metadata};
use manifest::assets;
pub use manifest::{download_manifest, manifest, Asset, DownloadAsset};
pub use paths::{data_dir, default_dir};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Serialize)]
pub struct Status {
    pub ready: bool,
    pub directory: PathBuf,
    pub missing: Vec<String>,
    pub download_bytes: u64,
}
/// Size-only UI check. Preparation and loading always verify the full SHA-256.
pub fn status(root: &Path) -> Status {
    let assets = assets();
    let mut missing = assets
        .iter()
        .filter(|a| {
            !fs::metadata(root.join(&a.model).join(&a.file))
                .is_ok_and(|m| m.is_file() && m.len() == a.size)
        })
        .map(|a| format!("{}/{}", a.model, a.file))
        .collect::<Vec<_>>();
    if !cfg!(feature = "offline-model") {
        missing.push("offline-model feature required".into());
    }
    Status {
        ready: missing.is_empty(),
        directory: root.to_owned(),
        missing,
        download_bytes: assets.iter().map(|a| a.size).sum(),
    }
}
pub fn prepare(root: &Path, progress: impl FnMut(u64, u64, &str)) -> Result<()> {
    prepare_with_ca(root, None, progress)
}
pub fn prepare_with_ca(
    root: &Path,
    ca: Option<&Path>,
    progress: impl FnMut(u64, u64, &str),
) -> Result<()> {
    anyhow::ensure!(
        cfg!(feature = "offline-model"),
        "이 실행 파일에는 내부 INT8 모델 기능이 없습니다"
    );
    download::prepare(root, assets(), ca, progress)?;
    verify(root)
}
/// Source download exists only in a maintainer build, never in shipping binaries.
#[cfg(feature = "model-tools")]
pub fn prepare_sources(root: &Path, progress: impl FnMut(u64, u64, &str)) -> Result<()> {
    let assets = manifest()
        .into_iter()
        .map(|a| DownloadAsset {
            url: format!(
                "https://huggingface.co/{}/resolve/{}/{}?download=true",
                a.repo, a.revision, a.file
            ),
            model: a.model,
            file: a.file,
            size: a.size,
            sha256: a.sha256,
            github_release_api: None,
        })
        .collect::<Vec<_>>();
    download::prepare(root, &assets, None, progress)
}

#[cfg(test)]
mod tests;
