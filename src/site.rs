use anyhow::{bail, Context, Result};
use scraper::{Html, Selector, ElementRef, Node};
use ego_tree::NodeRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Site { Kakuyomu, Alphapolis, Syosetu, Syosetu18, Hameln, Novelup }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Work { pub site: Site, pub url: String, pub title: String, pub description: String, pub episodes: Vec<String> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode { pub title: String, pub lines: Vec<String> }

pub fn canonicalize(input: &str) -> Result<(Site, String)> {
    let url = Url::parse(input).context("유효한 https URL이 필요합니다")?;
    if url.scheme() != "https" && url.scheme() != "http" { bail!("HTTP(S) URL만 허용됩니다"); }
    if url.username() != "" || url.password().is_some() { bail!("URL 자격 증명을 사용할 수 없습니다"); }
    let host = url.host_str().context("호스트가 없습니다")?.to_ascii_lowercase();
    let path: Vec<_> = url.path_segments().context("잘못된 URL 경로")?.filter(|x| !x.is_empty()).collect();
    let (site, path) = match host.as_str() {
        "kakuyomu.jp" if path.len() >= 2 && path[0] == "works" =>
            (Site::Kakuyomu, format!("/works/{}", path[1])),
        "www.alphapolis.co.jp" | "alphapolis.co.jp" if path.len() >= 3 && path[0] == "novel" =>
            (Site::Alphapolis, format!("/novel/{}/{}/", path[1], path[2])),
        "ncode.syosetu.com" if !path.is_empty() => (Site::Syosetu, format!("/{}/", path[0])),
        "novel18.syosetu.com" if !path.is_empty() => (Site::Syosetu18, format!("/{}/", path[0])),
        "syosetu.org" if path.len() >= 2 && path[0] == "novel" =>
            (Site::Hameln, format!("/novel/{}/", path[1])),
        "novelup.plus" if path.len() >= 2 && path[0] == "story" =>
            (Site::Novelup, format!("/story/{}", path[1])),
        _ => bail!("지원되지 않는 작품 URL: {}", input),
    };
    let host = match site { Site::Alphapolis => "www.alphapolis.co.jp", _ => host.as_str() };
    Ok((site, format!("https://{host}{path}")))
}
fn first_text(doc: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|s| {
        let sel = Selector::parse(s).ok()?;
        doc.select(&sel).next().map(|el| clean(&el.text().collect::<String>())).filter(|s| !s.is_empty())
    })
}
fn clean(input: &str) -> String { input.split_whitespace().collect::<Vec<_>>().join(" ") }
fn is_valid_episode(site: Site, original: &Url, candidate: &Url) -> bool {
    if candidate.scheme() != "https" && candidate.scheme() != "http" { return false; }
    if candidate.host_str() != original.host_str() { return false; }
    let path = candidate.path();
    match site {
        Site::Kakuyomu => path.starts_with(&format!("{}/episodes/", original.path().trim_end_matches('/'))),
        Site::Alphapolis => path.starts_with(&format!("{}episode/", original.path().trim_end_matches('/').to_string() + "/"))
            || (path.starts_with(original.path()) && path.contains("/episode/")),
        Site::Syosetu | Site::Syosetu18 => path.starts_with(original.path()) && path != original.path(),
        Site::Hameln => path.starts_with(original.path()) && path != original.path(),
        Site::Novelup => path.starts_with(original.path()) && path != original.path(),
    }
}
fn collect_links(site: Site, base: &Url, html: &Html) -> Vec<String> {
    let selectors: &[&str] = match site {
        Site::Kakuyomu => &["a[href*='/episodes/']"],
        Site::Alphapolis => &[".episode a[href]", ".episode-list a[href]", ".episode-title a[href]", "a[href*='/episode/']"],
        Site::Syosetu | Site::Syosetu18 => &[".p-eplist__sublist a[href]", "a[href]"],
        Site::Hameln => &[".episode-list__link[href]", "#maind a[href]"],
        Site::Novelup => &[".episodeTitle a[href]", ".episodeList a[href]", "a[href*='/episode/']"],
    };
    let mut found = Vec::new(); let mut seen = HashSet::new();
    for selector in selectors {
        let Ok(selector) = Selector::parse(selector) else { continue; };
        for anchor in html.select(&selector) {
            let Some(href) = anchor.value().attr("href") else { continue; };
            let Ok(link) = base.join(href) else { continue; };
            if is_valid_episode(site, base, &link) {
                let normalized = link.as_str().split('#').next().unwrap_or("").to_owned();
                if seen.insert(normalized.clone()) { found.push(normalized); }
            }
        }
        if !found.is_empty() { break; }
    }
    found
}
fn kakuyomu_next_data(document: &Html, work_url: &Url) -> Option<(String, String, Vec<String>)> {
    let selector = Selector::parse("#__NEXT_DATA__").ok()?;
    let raw = document.select(&selector).next()?.inner_html();
    let json: Value = serde_json::from_str(&raw).ok()?;
    let apollo = json.pointer("/props/pageProps/__APOLLO_STATE__")?;
    let work_id = work_url.path_segments()?.nth(1)?;
    let obj = apollo.as_object()?;
    let info = obj.iter().find(|(k, _)| k.starts_with(&format!("Work:{work_id}")))?.1;
    let title = info.get("title")?.as_str()?.to_owned();
    let desc = info.get("introduction").and_then(Value::as_str).unwrap_or("").to_owned();
    let mut links = Vec::new();
    if let Some(toc) = info.get("tableOfContents") { collect_episode_ids(toc, &mut links); }
    if links.is_empty() {
        for (key, item) in obj { if key.starts_with("Episode:") {
            if let Some(episode_id) = item.get("id").and_then(Value::as_str) { links.push(episode_id.to_owned()); }
        } }
    }
    let mut seen=HashSet::new();
    let links=links.into_iter().filter(|id| seen.insert(id.clone()))
        .map(|episode_id| format!("https://kakuyomu.jp/works/{work_id}/episodes/{episode_id}")).collect();
    Some((title, desc, links))
}
fn collect_episode_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(xs) => for x in xs { collect_episode_ids(x,out); },
        Value::Object(map) => {
            if let Some(id) = map.get("id").and_then(Value::as_str) { if id.chars().all(|c|c.is_ascii_digit()) { out.push(id.to_owned()); } }
            if let Some(reference) = map.get("__ref").and_then(Value::as_str) { if let Some(id)=reference.strip_prefix("Episode:") { out.push(id.to_owned()); } }
            for (key, x) in map { if key!="id" && key!="__ref" { collect_episode_ids(x,out); } }
        }
        _=>{}
    }
}
pub fn parse_work(url: &str, html: &str) -> Result<Work> {
    let (site, canonical) = canonicalize(url)?;
    let base = Url::parse(&canonical)?;
    let doc = Html::parse_document(html);
    let title_candidates: &[&str] = match site {
        Site::Kakuyomu => &["h1", "meta[property='og:title']"],
        Site::Alphapolis => &[".content-main .title", ".novel-title", "h1", ".title"],
        Site::Syosetu | Site::Syosetu18 => &[".p-novel__title", "h1", ".novel_title"],
        Site::Hameln => &["[itemprop='name']", "h1"],
        Site::Novelup => &[".storyTitle", "h1"],
    };
    let title=first_text(&doc,title_candidates)
        .or_else(|| (site==Site::Kakuyomu).then(|| kakuyomu_next_data(&doc,&base)).flatten().map(|v|v.0))
        .context("작품 제목을 찾지 못했습니다")?;
    let desc=first_text(&doc,&[".abstract", ".novel_synopsis", ".introduction", ".p-novel__summary", "meta[name='description']"]).unwrap_or_default();
    let mut episodes=collect_links(site,&base,&doc);
    if site==Site::Kakuyomu {
        if let Some((_,_,json_links))=kakuyomu_next_data(&doc,&base) {
            if episodes.is_empty() { episodes=json_links; }
        }
    }
    if episodes.is_empty() { bail!("목차 링크를 찾지 못했습니다. 인증/연령제한/동적 페이지 여부를 확인하세요"); }
    Ok(Work {site,url:canonical,title,description:desc,episodes})
}
fn extract_text(node: NodeRef<'_, Node>, out: &mut String, include_furigana: bool) {
    match node.value() {
        Node::Text(x) => out.push_str(x),
        Node::Element(e) => {
            let tag=e.name();
            if matches!(tag,"script"|"style"|"noscript") || (!include_furigana && matches!(tag,"rp"|"rt")) { return; }
            if tag=="br" {out.push('\n');return;}
            for child in node.children() { extract_text(child,out,include_furigana); }
            if matches!(tag, "p"|"div"|"section"|"li"|"h2"|"h3") {out.push('\n');}
        }
        _=>{for child in node.children(){extract_text(child,out,include_furigana);}}
    }
}
pub fn parse_episode(site: Site, html: &str, include_furigana: bool) -> Result<Episode> {
    let document=Html::parse_document(html);
    let (title_selectors,body_selectors):(&[&str],&[&str])=match site {
        Site::Kakuyomu => (&[".widget-episodeTitle", "h1"],&[".widget-episodeBody", "#contentMain .widget-episodeBody", "[data-testid='episode-body']"]),
        Site::Alphapolis => (&[".episode-title", ".novel-episode__title", "h1"],&["#novelBody", ".novel-body", ".novel-body__body", ".episode-body", "[itemprop='articleBody']"]),
        Site::Syosetu|Site::Syosetu18 => (&[".p-novel__title--rensai", ".novel_subtitle", "h1"],&[".js-novel-text.p-novel__text", "#novel_honbun", ".novel_view"]),
        Site::Hameln => (&[".ss", "h1"],&["#honbun"]),
        Site::Novelup => (&[".episode_title", "h1"],&["#js-scroll-area .content", ".content"]),
    };
    let title=first_text(&document,title_selectors).unwrap_or_default();
    let body: Option<ElementRef<'_>> = body_selectors.iter().find_map(|s| {
        let selector=Selector::parse(s).ok()?;
        document.select(&selector).find(|e| clean(&e.text().collect::<String>()).len()>1)
    });
    let body=body.context("작품 본문을 찾지 못했습니다(로그인·유료·차단·HTML 변경 가능성)")?;
    let mut text=String::new();
    extract_text(*body, &mut text, include_furigana);
    let lines:Vec<String>=text.replace("\r\n","\n").split('\n').map(str::trim_end).map(str::to_string).collect();
    if !lines.iter().any(|x| !x.trim().is_empty()) {bail!("빈 본문입니다");}
    Ok(Episode{title,lines})
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn canonical_validation(){
        assert_eq!(canonicalize("https://www.alphapolis.co.jp/novel/767156331/284095783").unwrap().1,
                   "https://www.alphapolis.co.jp/novel/767156331/284095783/");
        assert_eq!(canonicalize("https://kakuyomu.jp/works/2912051603596422650/episodes/123").unwrap().1,
                   "https://kakuyomu.jp/works/2912051603596422650");
        assert!(canonicalize("https://kakuyomu.jp.evil.test/works/1").is_err());
        assert!(canonicalize("file:///etc/passwd").is_err());
    }
    #[test] fn alphapolis_toc_and_episode(){
        let work=parse_work("https://www.alphapolis.co.jp/novel/767156331/284095783",
            "<div class='content-main'><h1 class='title'>作品名</h1></div><div class='episode'><a href='/novel/767156331/284095783/episode/123'>1話</a></div>").unwrap();
        assert_eq!(work.episodes.len(),1);
        let episode=parse_episode(Site::Alphapolis,
            "<h1 class='episode-title'>第一話</h1><div id='novelBody'><p>本文<ruby>漢<rt>かん</rt></ruby>字</p><p>次の行</p></div>",false).unwrap();
        assert_eq!(episode.title,"第一話"); assert!(episode.lines.join(" ").contains("本文漢字"));
        assert!(!episode.lines.join(" ").contains("かん"));
    }
    #[test] fn kakuyomu_toc(){
        let work=parse_work("https://kakuyomu.jp/works/2912051603596422650",
            "<h1>タイトル</h1><a href='/works/2912051603596422650/episodes/1'>A</a><a href='/works/2912051603596422650/episodes/2'>B</a>").unwrap();
        assert_eq!(work.episodes.len(),2);
    }
}