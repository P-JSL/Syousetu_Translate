//! Immutable model provenance and a separate INT8-only download manifest.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
#[path = "model_download.rs"]
mod download;

/// Maintainer provenance. The application never downloads these FP32 weights.
#[derive(Debug, Deserialize)]
pub struct Asset {
    pub model: String,
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
}
pub fn manifest() -> Vec<Asset> {
    serde_json::from_str(include_str!("../models/manifest.json")).expect("embedded source manifest")
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DownloadAsset {
    pub model: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_release_api: Option<String>,
}
pub fn download_manifest() -> Vec<DownloadAsset> {
    serde_json::from_str(include_str!("../models/downloads.json"))
        .expect("embedded INT8 download manifest")
}
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LOCALAPPDATA").filter(|p| !p.is_empty()) {
        return PathBuf::from(p).join("SyousetuTranslate");
    }
    if let Some(p) = std::env::var_os("XDG_DATA_HOME").filter(|p| !p.is_empty()) {
        return PathBuf::from(p).join("SyousetuTranslate");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/SyousetuTranslate")
}
pub fn default_dir() -> PathBuf {
    std::env::var_os("SYOUSETU_MODEL_DIR")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| bundled_dir(&exe))
        })
        .unwrap_or_else(|| data_dir().join("models"))
}
fn bundled_dir(executable: &Path) -> Option<PathBuf> {
    let candidate = executable.parent()?.join("models");
    status(&candidate).ready.then_some(candidate)
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub ready: bool,
    pub directory: PathBuf,
    pub missing: Vec<String>,
    pub download_bytes: u64,
}
/// Size-only UI check. Preparation and loading always verify the full SHA-256.
pub fn status(root: &Path) -> Status {
    let assets = download_manifest();
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
fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn verify_file(path: &Path, size: u64, sha256: &str) -> Result<()> {
    anyhow::ensure!(
        fs::metadata(path)
            .with_context(|| format!("모델 파일 없음: {}", path.display()))?
            .len()
            == size
            && hash_file(path)? == sha256,
        "모델 체크섬 불일치: {}",
        path.display()
    );
    Ok(())
}
pub fn verify_metadata(root: &Path) -> Result<()> {
    for a in download_manifest()
        .into_iter()
        .filter(|a| a.file != "model-int8.gguf")
    {
        verify_file(&root.join(a.model).join(a.file), a.size, &a.sha256)
            .context("모델 준비 / 복구를 실행해 주세요")?;
    }
    Ok(())
}
pub fn verify(root: &Path) -> Result<()> {
    verify_metadata(root)?;
    #[cfg(feature = "offline-model")]
    crate::int8::verify(&root.join("ja-ko"))?;
    #[cfg(not(feature = "offline-model"))]
    anyhow::bail!("이 실행 파일에는 내부 INT8 모델 기능이 없습니다");
    #[allow(unreachable_code)]
    Ok(())
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
    download::prepare(root, &download_manifest(), ca, progress)?;
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
mod tests {
    use super::*;
    #[test]
    fn direct_manifest_excludes_fp32_and_pins_the_existing_int8() {
        let assets = download_manifest();
        assert_eq!(assets.len(), 4);
        assert_eq!(assets.iter().map(|a| a.size).sum::<u64>(), 360_332_983);
        let mut paths = std::collections::HashSet::new();
        for a in &assets {
            assert!(paths.insert(format!("{}/{}", a.model, a.file)));
            assert!(!a.file.contains(['/', '\\']));
            assert!(!a.file.ends_with("safetensors"));
            assert!(a.url.starts_with("https://"));
            assert!(a.size > 0);
            assert_eq!(a.sha256.len(), 64);
            assert!(a.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
        #[cfg(feature = "offline-model")]
        {
            let spec = crate::int8::spec();
            let a = assets.iter().find(|a| a.file == spec.file).unwrap();
            assert_eq!(
                (a.size, a.sha256.as_str()),
                (spec.size, spec.sha256.as_str())
            );
        }
        let fresh = status(Path::new("intentionally-absent-model-test"));
        assert!(!fresh.ready);
        assert_eq!(fresh.download_bytes, 360_332_983);
    }
    #[test]
    fn provenance_is_pinned_but_not_a_runtime_download() {
        for a in manifest() {
            assert_eq!(a.revision.len(), 40);
            assert_eq!(a.sha256.len(), 64);
        }
    }
    #[cfg(feature = "offline-model")]
    #[test]
    fn portable_discovery_requires_the_complete_bundle_next_to_the_executable() {
        let root =
            std::env::temp_dir().join(format!("syousetu-portable-path-{}", std::process::id()));
        let exe = root.join("syousetu-translate.exe");
        assert!(bundled_dir(&exe).is_none());
        for asset in download_manifest() {
            let path = root.join("models").join(asset.model).join(asset.file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            File::create(path).unwrap().set_len(asset.size).unwrap();
        }
        assert_eq!(bundled_dir(&exe), Some(root.join("models")));
        fs::remove_file(root.join("models/ja-ko/vocab.json")).unwrap();
        assert!(bundled_dir(&exe).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
