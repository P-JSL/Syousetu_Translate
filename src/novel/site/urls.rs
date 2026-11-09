use super::Site;
use anyhow::{bail, Context, Result};
use url::Url;
pub fn canonicalize(input: &str) -> Result<(Site, String)> {
    let url = Url::parse(input).context("유효한 https URL이 필요합니다")?;
    if url.scheme() != "https" && url.scheme() != "http" {
        bail!("HTTP(S) URL만 허용됩니다");
    }
    if url.username() != "" || url.password().is_some() {
        bail!("URL 자격 증명을 사용할 수 없습니다");
    }
    let host = url
        .host_str()
        .context("호스트가 없습니다")?
        .to_ascii_lowercase();
    let path: Vec<_> = url
        .path_segments()
        .context("잘못된 URL 경로")?
        .filter(|x| !x.is_empty())
        .collect();
    let (site, path) = match host.as_str() {
        "kakuyomu.jp" if path.len() >= 2 && path[0] == "works" => {
            (Site::Kakuyomu, format!("/works/{}", path[1]))
        }
        "www.alphapolis.co.jp" | "alphapolis.co.jp" if path.len() >= 3 && path[0] == "novel" => {
            (Site::Alphapolis, format!("/novel/{}/{}/", path[1], path[2]))
        }
        "ncode.syosetu.com" if !path.is_empty() => {
            (Site::Syosetu, format!("/{}/", path[0].to_ascii_lowercase()))
        }
        "novel18.syosetu.com" if !path.is_empty() => (
            Site::Syosetu18,
            format!("/{}/", path[0].to_ascii_lowercase()),
        ),
        "syosetu.org" if path.len() >= 2 && path[0] == "novel" => {
            (Site::Hameln, format!("/novel/{}/", path[1]))
        }
        "novelup.plus" if path.len() >= 2 && path[0] == "story" => {
            (Site::Novelup, format!("/story/{}", path[1]))
        }
        _ => bail!("지원되지 않는 작품 URL: {}", input),
    };
    let host = match site {
        Site::Alphapolis => "www.alphapolis.co.jp",
        _ => host.as_str(),
    };
    Ok((site, format!("https://{host}{path}")))
}
pub(super) fn is_valid_episode(site: Site, original: &Url, candidate: &Url) -> bool {
    if candidate.scheme() != "https" && candidate.scheme() != "http" {
        return false;
    }
    if !candidate.username().is_empty()
        || candidate.password().is_some()
        || candidate.port().is_some()
    {
        return false;
    }
    let same_host = candidate.host_str() == original.host_str()
        || (site == Site::Alphapolis
            && matches!(
                candidate.host_str(),
                Some("alphapolis.co.jp" | "www.alphapolis.co.jp")
            ));
    if !same_host {
        return false;
    }
    let prefix = format!("{}/", original.path().trim_end_matches('/'));
    let Some(relative) = candidate.path().strip_prefix(&prefix) else {
        return false;
    };
    let numeric = |value: &str| !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit());
    match site {
        Site::Kakuyomu => relative
            .strip_prefix("episodes/")
            .is_some_and(|id| numeric(id.trim_end_matches('/'))),
        Site::Alphapolis => relative
            .strip_prefix("episode/")
            .is_some_and(|id| numeric(id.trim_end_matches('/'))),
        Site::Syosetu | Site::Syosetu18 => numeric(relative.trim_end_matches('/')),
        Site::Hameln => relative.strip_suffix(".html").is_some_and(numeric),
        Site::Novelup => {
            numeric(relative.trim_end_matches('/'))
                || relative
                    .strip_prefix("episode/")
                    .is_some_and(|id| numeric(id.trim_end_matches('/')))
        }
    }
}
