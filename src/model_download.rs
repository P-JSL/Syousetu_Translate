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
fn api_asset_url(client: &Client, api: &str, a: &DownloadAsset) -> Result<String> {
    // Build the asset endpoint from the pinned API origin, not a redirect URL
    // supplied by untrusted JSON. All downloaded bytes still need the pinned hash.
    let (base, _) = api
        .split_once("/releases/")
        .context("모델 릴리즈 API 주소 오류")?;
    let mut response = client
        .get(api)
        .header(header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(crate::http_client::error)?
        .error_for_status()?;
    anyhow::ensure!(
        !response.content_length().is_some_and(|n| n > 1024 * 1024),
        "모델 릴리즈 메타데이터 용량 초과"
    );
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 1024 * 1024,
        "모델 릴리즈 메타데이터 용량 초과"
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let matches = value["assets"]
        .as_array()
        .context("모델 릴리즈에 파일 목록이 없습니다")?
        .iter()
        .filter(|v| v["name"].as_str() == Some(&a.file))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        matches.len() == 1,
        "모델 릴리즈의 INT8 파일이 없거나 중복됩니다"
    );
    let entry = matches[0];
    anyhow::ensure!(
        entry["size"].as_u64() == Some(a.size),
        "릴리즈 모델 용량 불일치"
    );
    let id = entry["id"].as_u64().context("모델 자산 ID 없음")?;
    Ok(format!("{base}/releases/assets/{id}"))
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
fn install(temporary: &Path, path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}
fn transfer(
    client: &Client,
    url: &str,
    a: &DownloadAsset,
    path: &Path,
    temporary: &Path,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    let offset = fs::metadata(temporary).map(|m| m.len()).unwrap_or(0);
    let offset = if offset >= a.size {
        let _ = fs::remove_file(temporary);
        0
    } else {
        offset
    };
    let mut request = client
        .get(url)
        .header(header::ACCEPT, "application/octet-stream");
    if offset > 0 {
        request = request.header(header::RANGE, format!("bytes={offset}-"));
    }
    let response = request
        .send()
        .map_err(crate::http_client::error)?
        .error_for_status()?;
    write_response(response, a, path, temporary, offset, &mut progress)
}
fn write_response(
    mut response: Response,
    a: &DownloadAsset,
    path: &Path,
    temporary: &Path,
    requested: u64,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    let offset = if response.status() == StatusCode::PARTIAL_CONTENT {
        let expected = format!("bytes {requested}-{}/{}", a.size - 1, a.size);
        anyhow::ensure!(
            response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                == Some(&expected),
            "모델 재개 응답 범위 불일치"
        );
        requested
    } else {
        anyhow::ensure!(
            response.status() == StatusCode::OK,
            "모델 다운로드 응답 오류"
        );
        0
    };
    anyhow::ensure!(
        !response
            .content_length()
            .is_some_and(|n| n > a.size - offset),
        "모델 응답 용량 초과"
    );
    let mut hash = Sha256::new();
    if offset > 0 {
        let mut prefix = File::open(temporary)?;
        let mut buffer = [0u8; 128 * 1024];
        loop {
            let n = prefix.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(temporary)?;
    let mut count = offset;
    let mut buffer = [0u8; 128 * 1024];
    progress(count);
    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > a.size {
            drop(file);
            let _ = fs::remove_file(temporary);
            anyhow::bail!("모델 응답 용량 초과");
        }
        file.write_all(&buffer[..n])?;
        hash.update(&buffer[..n]);
        progress(count);
    }
    file.sync_all()?;
    drop(file);
    if count != a.size {
        anyhow::bail!("모델 다운로드가 중단되었습니다 ({count}/{})", a.size);
    }
    if format!("{:x}", hash.finalize()) != a.sha256 {
        fs::remove_file(temporary)?;
        anyhow::bail!("모델 다운로드 체크섬 불일치");
    }
    debug_assert_eq!(
        hash_file(temporary).ok().as_deref(),
        Some(a.sha256.as_str())
    );
    install(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex,
        },
        thread,
    };
    static ID: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: std::path::PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "syousetu-direct-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn asset(url: String, body: &[u8]) -> DownloadAsset {
        DownloadAsset {
            model: "ja-ko".into(),
            file: "model-int8.gguf".into(),
            size: body.len() as u64,
            sha256: format!("{:x}", Sha256::digest(body)),
            url,
            github_release_api: None,
        }
    }
    fn server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        let task = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    request.push_str(&line);
                }
                log.lock().unwrap().push(request);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (format!("http://{address}"), requests, task)
    }
    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
    fn client() -> Client {
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    }
    #[test]
    fn resumed_download_uses_range_and_verifies_the_complete_file() {
        let f = Fixture::new();
        let path = f.root.join("model-int8.gguf");
        let part = path.with_extension("part");
        fs::write(&part, b"abc").unwrap();
        let (url,requests,task)=server(vec!["HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 3-5/6\r\nContent-Length: 3\r\nConnection: close\r\n\r\ndef".into()]);
        let a = asset(url, b"abcdef");
        fetch(&client(), &a, &path, |_| {}).unwrap();
        task.join().unwrap();
        assert_eq!(fs::read(path).unwrap(), b"abcdef");
        assert!(!part.exists());
        assert!(requests.lock().unwrap()[0]
            .to_ascii_lowercase()
            .contains("range: bytes=3-"));
    }
    #[test]
    fn ignored_range_restarts_and_corrupt_response_cannot_replace_existing_file() {
        let f = Fixture::new();
        let path = f.root.join("model-int8.gguf");
        let part = path.with_extension("part");
        fs::write(&part, b"abc").unwrap();
        let (url, _, task) = server(vec![ok("abcdef")]);
        fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).unwrap();
        task.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abcdef");
        let (url, _, task) = server(vec![ok("broken")]);
        assert!(fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).is_err());
        task.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abcdef");
        assert!(!part.exists());
    }
    #[test]
    fn interrupted_download_keeps_prefix_for_retry_and_wrong_range_is_rejected() {
        let f = Fixture::new();
        let path = f.root.join("model-int8.gguf");
        let part = path.with_extension("part");
        let (url, _, task) = server(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc".into(),
        ]);
        assert!(fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).is_err());
        task.join().unwrap();
        assert_eq!(fs::read(&part).unwrap(), b"abc");
        assert!(!path.exists());
        let(url,_,task)=server(vec!["HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-2/6\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc".into()]);
        assert!(fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).is_err());
        task.join().unwrap();
        assert_eq!(fs::read(part).unwrap(), b"abc");
    }
    #[test]
    fn api_fallback_needs_no_token_and_never_requests_fp32() {
        let f = Fixture::new();
        let path = f.root.join("model-int8.gguf");
        let (url, requests, task) = server(vec![
            "HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            ok(r#"{"assets":[{"id":17,"name":"model-int8.gguf","size":6}]}"#),
            ok("abcdef"),
        ]);
        let mut a = asset(format!("{url}/direct-int8"), b"abcdef");
        a.github_release_api = Some(format!("{url}/repos/test/model/releases/tags/v1"));
        fetch(&client(), &a, &path, |_| {}).unwrap();
        task.join().unwrap();
        let calls = requests.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls[2].starts_with("GET /repos/test/model/releases/assets/17 "));
        for request in calls.iter() {
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(!request.contains("model.safetensors"));
        }
        assert_eq!(fs::read(path).unwrap(), b"abcdef");
    }
    #[test]
    fn completed_partial_is_installed_without_network() {
        let f = Fixture::new();
        let path = f.root.join("model-int8.gguf");
        fs::write(path.with_extension("part"), b"abcdef").unwrap();
        fetch(
            &client(),
            &asset("http://127.0.0.1:1/unreachable".into(), b"abcdef"),
            &path,
            |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(path).unwrap(), b"abcdef");
    }
}
