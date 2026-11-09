use super::dom::{first_element, select};
use super::urls::is_valid_episode;
use super::Site;
use super::{canonicalize, kakuyomu::episode_id};
use anyhow::{bail, Context, Result};
use ego_tree::NodeRef;
use scraper::{Html, Node};
use serde_json::Value;
use std::collections::HashSet;
use url::Url;
pub(super) struct AlphapolisData {
    pub(super) title: String,
    pub(super) episodes: Vec<String>,
}
pub(super) fn alphapolis_cover_data(document: &Html, base: &Url) -> Result<Option<AlphapolisData>> {
    let selector = select("script#app-cover-data[type='application/json'], #app-cover-episode script[type='application/json']")
        .expect("static selector");
    let mut scripts = document.select(&selector);
    let Some(script) = scripts.next() else {
        return Ok(None);
    };
    if scripts.next().is_some() {
        bail!("알파폴리스 목차 JSON이 여러 개여서 작품을 확정할 수 없습니다");
    }
    let data: Value =
        serde_json::from_str(&script.inner_html()).context("알파폴리스 목차 JSON 해석 실패")?;
    let content = data
        .get("content")
        .context("알파폴리스 작품 정보가 없습니다")?;
    let content_url = content
        .get("url")
        .and_then(Value::as_str)
        .context("알파폴리스 작품 URL이 없습니다")?;
    let content_url = base
        .join(content_url)
        .context("알파폴리스 작품 URL 해석 실패")?;
    if canonicalize(content_url.as_str())?.1 != base.as_str()
        || content.get("isNovel").and_then(Value::as_bool) != Some(true)
    {
        bail!("알파폴리스 목차 JSON이 요청한 소설의 정보와 다릅니다");
    }
    if data.get("isPartialEpisode").and_then(Value::as_bool) != Some(false)
        || data.get("isPreview").and_then(Value::as_bool) != Some(false)
    {
        bail!("알파폴리스 공개 목차가 일부만 제공되었거나 미리보기 상태입니다");
    }
    let title = content
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .context("알파폴리스 작품 제목이 없습니다")?
        .to_owned();
    let chapters = data
        .get("chapterEpisodes")
        .and_then(Value::as_array)
        .context("알파폴리스 챕터 목차가 없습니다")?;
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    for chapter in chapters {
        // Rental collections and unknown access flags do not grant public access.
        if chapter.get("isRental").and_then(Value::as_bool) != Some(false)
            || !chapter.get("rentalBookId").is_some_and(Value::is_null)
            || !chapter.get("book").is_some_and(Value::is_null)
        {
            continue;
        }
        if chapter
            .get("extraEpisode")
            .is_some_and(|extra| !extra.is_null())
        {
            bail!("알파폴리스 추가 에피소드 목차 형식을 확인할 수 없습니다");
        }
        let episodes = chapter
            .get("episodes")
            .and_then(Value::as_array)
            .context("알파폴리스 에피소드 목차가 없습니다")?;
        let mut chapter_links = Vec::new();
        let mut orders = HashSet::new();
        for episode in episodes {
            if episode.get("isPublic").and_then(Value::as_bool) != Some(true)
                || !episode.get("rental").is_some_and(Value::is_null)
            {
                continue;
            }
            let href = episode
                .get("url")
                .and_then(Value::as_str)
                .context("알파폴리스 공개 에피소드 URL이 없습니다")?;
            let mut link = base
                .join(href)
                .context("알파폴리스 에피소드 URL 해석 실패")?;
            if !is_valid_episode(Site::Alphapolis, base, &link) {
                bail!("알파폴리스 공개 에피소드 URL이 요청한 작품과 다릅니다");
            }
            let id = episode
                .get("episodeNo")
                .and_then(episode_id)
                .context("알파폴리스 에피소드 번호가 없습니다")?;
            if link.path().trim_end_matches('/').rsplit('/').next() != Some(id.as_str()) {
                bail!("알파폴리스 에피소드 번호와 URL이 일치하지 않습니다");
            }
            let order = episode
                .get("dispOrder")
                .and_then(Value::as_u64)
                .filter(|order| *order > 0)
                .context("알파폴리스 회차 순서를 확인할 수 없습니다")?;
            if !orders.insert(order) {
                bail!("알파폴리스 회차 순서가 중복되었습니다");
            }
            link.set_fragment(None);
            link.set_query(None);
            let _ = link.set_scheme("https");
            let _ = link.set_host(Some("www.alphapolis.co.jp"));
            chapter_links.push((order, link.to_string()));
        }
        chapter_links.sort_by_key(|(order, _)| *order);
        links.extend(
            chapter_links
                .into_iter()
                .map(|(_, link)| link)
                .filter(|link| seen.insert(link.clone())),
        );
    }
    Ok(Some(AlphapolisData {
        title,
        episodes: links,
    }))
}
pub(super) fn source_table_text(node: NodeRef<'_, Node>, out: &mut String) {
    if let Node::Text(text) = node.value() {
        out.push_str(text);
        return;
    }
    if let Node::Element(element) = node.value() {
        if element.classes().any(|class| class == "line-number") {
            return;
        }
    }
    for child in node.children() {
        source_table_text(child, out);
    }
    if matches!(node.value(), Node::Element(element) if element.name() == "tr") {
        out.push('\n');
    }
}
pub(super) fn alphapolis_source_table(document: &Html, body_selectors: &[&str]) -> Option<Html> {
    let selector = select("tbody").ok()?;
    for table in document.select(&selector) {
        let mut source = String::new();
        source_table_text(*table, &mut source);
        let parsed = Html::parse_document(&source);
        if first_element(&parsed, body_selectors).is_some() {
            return Some(parsed);
        }
    }
    None
}
