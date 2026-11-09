use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self},
    path::Path,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct Spec {
    pub format_version: u32,
    pub file: String,
    pub size: u64,
    pub sha256: String,
}
pub fn spec() -> Spec {
    serde_json::from_str(include_str!("../../models/int8.json")).expect("embedded INT8 spec")
}
pub(super) fn source_hash() -> String {
    crate::models::manifest()
        .into_iter()
        .find(|a| a.file == "model.safetensors")
        .expect("source model")
        .sha256
}
pub use crate::storage::sha256_file as hash;
pub fn exists(directory: &Path) -> bool {
    let spec = spec();
    spec.size > 0
        && fs::metadata(directory.join(spec.file))
            .is_ok_and(|m| m.is_file() && m.len() == spec.size)
}
pub fn verify(directory: &Path) -> Result<()> {
    let spec = spec();
    let path = directory.join(&spec.file);
    anyhow::ensure!(spec.format_version == 1, "지원하지 않는 INT8 모델 형식");
    anyhow::ensure!(
        exists(directory),
        "SMaLL-100 INT8 파일 준비가 필요합니다: {}",
        path.display()
    );
    anyhow::ensure!(
        hash(&path)? == spec.sha256,
        "INT8 모델 체크섬 불일치: {}",
        path.display()
    );
    Ok(())
}
