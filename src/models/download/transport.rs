use super::*;
pub(super) fn install(temporary: &Path, path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}
pub(super) fn transfer(
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
