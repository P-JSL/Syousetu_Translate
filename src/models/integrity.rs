use super::download_manifest;
use crate::storage::sha256_file as hash_file;
use anyhow::{Context, Result};
use std::{fs, path::Path};
pub(super) fn verify_file(path: &Path, size: u64, sha256: &str) -> Result<()> {
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
