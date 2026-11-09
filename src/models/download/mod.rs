//! Streaming, resumable INT8 transport. No credentials or FP32 fallback.
use super::{hash_file, verify_file, DownloadAsset};
use anyhow::{Context, Result};
use fs2::FileExt;
use reqwest::{
    blocking::{Client, Response},
    header, StatusCode,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::Duration,
};

mod github;
mod transport;
use github::api_asset_url;
use transport::{install, transfer};
pub(super) fn prepare(
    root: &Path,
    assets: &[DownloadAsset],
    ca: Option<&Path>,
    mut progress: impl FnMut(u64, u64, &str),
) -> Result<()> {
    fs::create_dir_all(root)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".prepare.lock"))?;
    lock.try_lock_exclusive()
        .context("다른 작업에서 모델을 준비하고 있습니다")?;
    let total = assets.iter().map(|a| a.size).sum();
    let mut done = 0;
    let mut client = None;
    for a in assets {
        let path = root.join(&a.model).join(&a.file);
        let label = format!("{}/{}", a.model, a.file);
        if verify_file(&path, a.size, &a.sha256).is_err() {
            if client.is_none() {
                client = Some(
                    crate::http_client::builder(ca)?
                        .https_only(true)
                        .redirect(reqwest::redirect::Policy::limited(10))
                        .connect_timeout(Duration::from_secs(30))
                        .timeout(Duration::from_secs(1800))
                        .user_agent("SyousetuTranslate/INT8-direct-download")
                        .build()?,
                );
            }
            fetch(client.as_ref().unwrap(),a,&path,|n|progress(done+n,total,&label))
                .with_context(||format!("INT8 모델 준비 실패: {label}. 재시도하면 검증된 파일과 부분 다운로드를 재사용합니다"))?;
        }
        done += a.size;
        progress(done, total, &label);
    }
    Ok(())
}
fn fetch(
    client: &Client,
    a: &DownloadAsset,
    path: &Path,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    fs::create_dir_all(path.parent().context("모델 경로 없음")?)?;
    let temporary = path.with_extension("part");
    if verify_file(&temporary, a.size, &a.sha256).is_ok() {
        progress(a.size);
        return install(&temporary, path);
    }
    let first = transfer(client, &a.url, a, path, &temporary, &mut progress);
    if first.is_ok() {
        return first;
    }
    if let Some(api) = &a.github_release_api {
        eprintln!("모델 다운로드의 GitHub API 대체 경로를 확인합니다...");
        let url = api_asset_url(client, api, a)
            .with_context(|| format!("기본 다운로드 오류: {:#}", first.unwrap_err()))?;
        return transfer(client, &url, a, path, &temporary, progress);
    }
    first
}

#[cfg(test)]
mod tests;
