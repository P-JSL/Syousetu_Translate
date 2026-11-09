//! Pinned public model assets. Only preparation performs network requests.
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

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
    serde_json::from_str(include_str!("../models/manifest.json")).expect("embedded model manifest")
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
        .unwrap_or_else(|| data_dir().join("models"))
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub ready: bool,
    pub directory: PathBuf,
    pub missing: Vec<String>,
    pub download_bytes: u64,
}
/// Fast UI check; loading separately verifies every SHA-256, including cached weights.
pub fn status(root: &Path) -> Status {
    let assets = manifest();
    let mut missing = assets
        .iter()
        .filter(|a| a.file != "model.safetensors")
        .filter(|a| {
            fs::metadata(root.join(&a.model).join(&a.file))
                .map(|m| m.len() != a.size || !m.is_file())
                .unwrap_or(true)
        })
        .map(|a| format!("{}/{}", a.model, a.file))
        .collect::<Vec<_>>();
    #[cfg(feature="offline-model")]
    if !crate::int8::exists(&root.join("ja-ko")) {missing.push("ja-ko/model-int8.gguf".into());}
    #[cfg(not(feature="offline-model"))]
    missing.push("offline-model feature required".into());
    Status {
        ready: missing.is_empty(),
        directory: root.to_owned(),
        missing,
        download_bytes: assets.iter().map(|a| a.size).sum(),
    }
}
fn hash_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn verify_metadata(root: &Path) -> Result<()> {
    for asset in manifest().into_iter().filter(|a|a.file != "model.safetensors") {
        verify_asset(root, &asset).with_context(||
        "모델 준비가 필요합니다. GUI의 '모델 준비 / 복구' 또는 models prepare 명령을 실행해 주세요")?;
    }
    Ok(())
}
pub fn verify(root:&Path)->Result<()> {
    verify_metadata(root)?;
    #[cfg(feature="offline-model")]
    crate::int8::verify(&root.join("ja-ko"))?;
    #[cfg(not(feature="offline-model"))]
    bail!("이 실행 파일에는 내부 INT8 모델 기능이 없습니다");
    #[allow(unreachable_code)]
    Ok(())
}
fn verify_asset(root: &Path, a: &Asset) -> Result<()> {
    let path = root.join(&a.model).join(&a.file);
    if fs::metadata(&path)
        .with_context(|| format!("모델 파일 없음: {}", path.display()))?
        .len()
        != a.size
        || hash_file(&path)? != a.sha256
    {
        bail!("모델 체크섬 불일치: {}", path.display());
    }
    Ok(())
}
pub fn prepare(root: &Path, mut progress: impl FnMut(u64, u64, &str)) -> Result<()> {
    prepare_with_ca(root,None,&mut progress)
}
pub fn prepare_with_ca(root:&Path,ca:Option<&Path>,mut progress:impl FnMut(u64,u64,&str))->Result<()>{
    fs::create_dir_all(root)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".prepare.lock"))?;
    lock.try_lock_exclusive()
        .context("다른 작업에서 모델을 준비하고 있습니다")?;
    #[cfg(feature="offline-model")]
    let int8_ready=crate::int8::verify(&root.join("ja-ko")).is_ok();
    #[cfg(not(feature="offline-model"))]
    let int8_ready=false;
    let assets = manifest().into_iter().filter(|a|a.file!="model.safetensors" || !int8_ready).collect::<Vec<_>>();
    let total = assets.iter().map(|a| a.size).sum();
    let mut done = 0u64;
    let builder = crate::http_client::builder(ca)?
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(1800))
        .user_agent("SyousetuTranslate/model-preparation");
    let client = builder.build()?;
    for asset in assets {
        let label = format!("{}/{}", asset.model, asset.file);
        if verify_asset(root, &asset).is_ok() {
            done += asset.size;
            progress(done, total, &label);
            continue;
        }
        let path = root.join(&asset.model).join(&asset.file);
        fs::create_dir_all(path.parent().context("모델 경로 없음")?)?;
        let temporary = path.with_extension("part");
        let result = (|| -> Result<()> {
            let url = format!(
                "https://huggingface.co/{}/resolve/{}/{}?download=true",
                asset.repo, asset.revision, asset.file
            );
            let mut response = client.get(url).send().map_err(crate::http_client::error)?.error_for_status()?;
            let mut file = File::create(&temporary)?;
            let mut hash = Sha256::new();
            let mut count = 0;
            let mut buffer = [0u8; 128 * 1024];
            loop {
                let n = response.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                count += n as u64;
                if count > asset.size {
                    bail!("모델 응답 용량 초과: {label}");
                }
                file.write_all(&buffer[..n])?;
                hash.update(&buffer[..n]);
                progress(done + count, total, &label);
            }
            if count != asset.size || format!("{:x}", hash.finalize()) != asset.sha256 {
                bail!("모델 다운로드 체크섬 불일치: {label}");
            }
            file.sync_all()?;
            drop(file);
            // All readers have finished hashing before preparation replaces a bad file.
            if path.exists() {
                fs::remove_file(&path)?;
            }
            fs::rename(&temporary, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.with_context(|| {
            format!("모델 준비 실패: {label}. 다시 준비하면 검증된 파일은 재사용합니다")
        })?;
        done += asset.size;
    }
    #[cfg(feature="offline-model")]
    if !int8_ready {
        progress(total,total,"INT8 변환 및 저장 중 (최초 1회)");
        crate::int8::ensure(&root.join("ja-ko"))?;
    }
    verify(root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_assets_have_unique_paths_and_complete_integrity_metadata() {
        let assets = manifest();
        let mut paths = std::collections::HashSet::new();
        for asset in &assets {
            assert!(paths.insert(format!("{}/{}", asset.model, asset.file)));
            assert!(asset.size > 0);
            assert_eq!(asset.revision.len(), 40);
            assert!(asset.revision.chars().all(|c| c.is_ascii_hexdigit()));
            assert_eq!(asset.sha256.len(), 64);
            assert!(asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!asset.file.contains(['/', '\\']));
        }
        assert_eq!(assets.len(), 4);
    }
    #[test]
    fn cached_files_are_not_trusted_by_length_alone() {
        let root = std::env::temp_dir().join(format!("syousetu-hash-{}", std::process::id()));
        fs::create_dir_all(root.join("test")).unwrap();
        let asset = Asset {
            model: "test".into(),
            repo: "unused".into(),
            revision: "unused".into(),
            file: "x".into(),
            size: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        };
        fs::write(root.join("test/x"), b"abc").unwrap();
        assert!(verify_asset(&root, &asset).is_ok());
        fs::write(root.join("test/x"), b"bad").unwrap();
        assert!(verify_asset(&root, &asset).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
