use super::*;
pub(super) fn api_asset_url(client: &Client, api: &str, a: &DownloadAsset) -> Result<String> {
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
