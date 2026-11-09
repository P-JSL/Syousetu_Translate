use serde::{Deserialize, Serialize};
/// Maintainer provenance. The application never downloads these FP32 weights.
#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub model: String,
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
}
pub fn manifest() -> Vec<Asset> {
    static SOURCE: std::sync::OnceLock<Vec<Asset>> = std::sync::OnceLock::new();
    SOURCE
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../models/manifest.json"))
                .expect("embedded source manifest")
        })
        .clone()
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
    assets().to_vec()
}
pub(super) fn assets() -> &'static [DownloadAsset] {
    static ASSETS: std::sync::OnceLock<Vec<DownloadAsset>> = std::sync::OnceLock::new();
    ASSETS.get_or_init(|| {
        serde_json::from_str(include_str!("../../models/downloads.json"))
            .expect("embedded INT8 download manifest")
    })
}
