use anyhow::{bail, Context, Result};
use ego_tree::NodeRef;
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Site {
    Kakuyomu,
    Alphapolis,
    Syosetu,
    Syosetu18,
    Hameln,
    Novelup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Work {
    pub site: Site,
    pub url: String,
    pub title: String,
    pub description: String,
    pub episodes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub title: String,
    pub lines: Vec<String>,
}

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
fn first_text(doc: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|s| {
        let sel = Selector::parse(s).ok()?;
        doc.select(&sel).find_map(|el| {
            el.value()
                .attr("content")
                .map(clean)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    let value = clean(&el.text().collect::<String>());
                    (!value.is_empty()).then_some(value)
                })
        })
    })
}
fn clean(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// Some public pages serve only a preview of the TOC. Never silently
/// present such a partial index as a complete work.
fn advertised_episode_count(document: &Html) -> Option<usize> {
    let mut text = String::new();
    extract_text(*document.root_element(), &mut text, false);
    let count = regex::Regex::new(r"全\s*([0-9,]+)\s*(?:話|エピソード)").ok()?;
    let raw = count.captures(&text)?.get(1)?.as_str().replace(',', "");
    raw.parse().ok()
}
fn is_valid_episode(site: Site, original: &Url, candidate: &Url) -> bool {
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
fn collect_links(site: Site, base: &Url, html: &Html) -> Vec<String> {
    let selectors: &[&str] = match site {
        Site::Kakuyomu => &["a[href*='/episodes/']"],
        Site::Alphapolis => &[
            ".episode a[href]",
            ".episode-list a[href]",
            ".episode-title a[href]",
            "a[href*='/episode/']",
        ],
        Site::Syosetu | Site::Syosetu18 => &[".p-eplist__sublist a[href]", "a[href]"],
        Site::Hameln => &[".episode-list__link[href]", "#maind a[href]"],
        Site::Novelup => &[
            ".episodeList .episodeTitle[href], .episodeList .episodeTitle a[href]",
            "#section_episode .episode_link a[href]",
            ".episodeTitle[href], .episodeTitle a[href]",
            ".episodeList a[href]",
            "a[href*='/episode/']",
        ],
    };
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for selector in selectors {
        let Ok(selector) = Selector::parse(selector) else {
            continue;
        };
        for anchor in html.select(&selector) {
            let Some(href) = anchor.value().attr("href") else {
                continue;
            };
            let Ok(mut link) = base.join(href) else {
                continue;
            };
            if is_valid_episode(site, base, &link) {
                link.set_fragment(None);
                link.set_query(None);
                let _ = link.set_scheme("https");
                if site == Site::Alphapolis {
                    let _ = link.set_host(Some("www.alphapolis.co.jp"));
                }
                let normalized = link.to_string();
                if seen.insert(normalized.clone()) {
                    found.push(normalized);
                }
            }
        }
        if !found.is_empty() {
            break;
        }
    }
    found
}
struct KakuyomuData {
    title: String,
    description: String,
    episodes: Vec<String>,
    episode_count: Option<usize>,
    unresolved_toc: bool,
}
fn episode_id(value: &Value) -> Option<String> {
    let id = match value {
        Value::String(id) => id.clone(),
        Value::Number(id) => id.as_u64()?.to_string(),
        _ => return None,
    };
    (!id.is_empty() && id.bytes().all(|c| c.is_ascii_digit())).then_some(id)
}
fn kakuyomu_next_data(document: &Html, work_url: &Url) -> Option<KakuyomuData> {
    let selector = Selector::parse("#__NEXT_DATA__").ok()?;
    let raw = document.select(&selector).next()?.inner_html();
    let json: Value = serde_json::from_str(&raw).ok()?;
    let apollo = json.pointer("/props/pageProps/__APOLLO_STATE__")?;
    let work_id = work_url.path_segments()?.nth(1)?;
    let obj = apollo.as_object()?;
    let info = obj.get(&format!("Work:{work_id}")).or_else(|| {
        obj.iter().find_map(|(key, value)| {
            let key = key.strip_prefix("Work:")?;
            let identity: Value = serde_json::from_str(key).ok()?;
            (identity.get("id").and_then(Value::as_str) == Some(work_id)).then_some(value)
        })
    })?;
    let title = info.get("title")?.as_str()?.to_owned();
    let desc = info
        .get("introduction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut links = Vec::new();
    let episode_count = info
        .get("publicEpisodeCount")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok());
    let unresolved_toc = if let Some(toc) = info.get("tableOfContentsV2") {
        info.get("__typename").and_then(Value::as_str) != Some("Work")
            || info.get("id").and_then(Value::as_str) != Some(work_id)
            || !collect_episode_ids_v2(toc, obj, &mut links)
    } else {
        info.get("tableOfContents").is_some_and(|toc| {
            !collect_episode_ids(toc, obj, &mut links, &mut HashSet::new(), false, 0)
        })
    } || (info.get("publicEpisodeCount").is_some() && episode_count.is_none());
    let mut seen = HashSet::new();
    let links = links
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .map(|episode_id| format!("https://kakuyomu.jp/works/{work_id}/episodes/{episode_id}"))
        .collect();
    Some(KakuyomuData {
        title,
        description: desc,
        episodes: links,
        episode_count,
        unresolved_toc,
    })
}
fn kakuyomu_typed_node<'a>(
    value: &'a Value,
    state: &'a serde_json::Map<String, Value>,
    typename: &str,
) -> Option<&'a Value> {
    let node = if let Some(reference) = value.get("__ref").and_then(Value::as_str) {
        let id = reference.strip_prefix(&format!("{typename}:"))?;
        let node = state.get(reference)?;
        if episode_id(node.get("id")?)?.as_str() != id {
            return None;
        }
        node
    } else {
        value
    };
    if node.get("__typename").and_then(Value::as_str) != Some(typename)
        || episode_id(node.get("id")?).is_none()
    {
        return None;
    }
    Some(node)
}
/// V2 explicitly orders TableOfContentsChapter nodes and their Episode unions.
/// Resolve only those defined types, never unrelated objects in the Apollo cache.
fn collect_episode_ids_v2(
    value: &Value,
    state: &serde_json::Map<String, Value>,
    out: &mut Vec<String>,
) -> bool {
    let Some(chapters) = value.as_array() else {
        return false;
    };
    let mut complete = true;
    for chapter in chapters {
        let Some(chapter) = kakuyomu_typed_node(chapter, state, "TableOfContentsChapter") else {
            complete = false;
            continue;
        };
        let Some(episodes) = chapter.get("episodeUnions").and_then(Value::as_array) else {
            complete = false;
            continue;
        };
        for episode in episodes {
            let Some(episode) = kakuyomu_typed_node(episode, state, "Episode") else {
                complete = false;
                continue;
            };
            if episode.get("isPublic").and_then(Value::as_bool) == Some(false) {
                complete = false;
                continue;
            }
            if let Some(id) = episode.get("id").and_then(episode_id) {
                out.push(id);
            }
        }
    }
    complete
}
/// Follow the selected work's TOC references in array order. A numeric chapter
/// ID or an unrelated cached Episode object is not an episode in this work.
fn collect_episode_ids(
    value: &Value,
    state: &serde_json::Map<String, Value>,
    out: &mut Vec<String>,
    visiting: &mut HashSet<String>,
    episode_context: bool,
    depth: usize,
) -> bool {
    if depth > 64 {
        return false;
    }
    match value {
        Value::Array(xs) => {
            // Visit every node even after a failure: collection has side effects.
            let mut complete = true;
            for x in xs {complete = collect_episode_ids(x,state,out,visiting,episode_context,depth+1) && complete;}
            complete
        },
        Value::Object(map) => {
            if let Some(reference) = map.get("__ref").and_then(Value::as_str) {
                if let Some(id) = reference.strip_prefix("Episode:") {
                    let id = state
                        .get(reference)
                        .and_then(|value| value.get("id"))
                        .and_then(episode_id)
                        .or_else(|| episode_id(&Value::String(id.to_owned())));
                    if let Some(id) = id {
                        out.push(id);
                        return true;
                    }
                    return false;
                }
                if !visiting.insert(reference.to_owned()) {
                    return false;
                }
                let complete = state.get(reference).is_some_and(|target| {
                    collect_episode_ids(target, state, out, visiting, episode_context, depth + 1)
                });
                visiting.remove(reference);
                return complete;
            }
            let typename = map.get("__typename").and_then(Value::as_str);
            if typename == Some("Episode") || (episode_context && typename.is_none()) {
                if let Some(id) = map.get("id").and_then(episode_id) {
                    out.push(id);
                    return true;
                }
                if typename == Some("Episode") {
                    return false;
                }
            }
            let before = out.len();
            let complete = map
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "id" | "__typename" | "chapter"))
                .fold(true, |complete, (key, child)| {
                    let context = matches!(key.as_str(), "episode" | "episodes" | "episodeUnions");
                    collect_episode_ids(child, state, out, visiting, context, depth + 1) && complete
                });
            complete && (!episode_context || out.len() > before)
        }
        _ => !episode_context,
    }
}
struct AlphapolisData {
    title: String,
    episodes: Vec<String>,
}
fn alphapolis_cover_data(document: &Html, base: &Url) -> Result<Option<AlphapolisData>> {
    let selector = Selector::parse("script#app-cover-data[type='application/json'], #app-cover-episode script[type='application/json']")
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
fn total_episode_count(site: Site, doc: &Html) -> Option<usize> {
    if site == Site::Novelup {
        let text = first_text(doc, &[".totalEpisode", ".total_episode_num"])?;
        let number = regex::Regex::new(r"[0-9][0-9,]*").ok()?;
        return number.find(&text)?.as_str().replace(',', "").parse().ok();
    }
    if matches!(site, Site::Kakuyomu | Site::Syosetu | Site::Syosetu18) {
        return advertised_episode_count(doc);
    }
    None
}
fn index_page_numbers(site: Site, base: &Url, doc: &Html) -> Result<BTreeSet<usize>> {
    let mut pages = BTreeSet::new();
    if !matches!(site, Site::Novelup | Site::Syosetu | Site::Syosetu18) {
        return Ok(pages);
    }
    if site == Site::Novelup {
        // Both supplied Java versions use 100 episodes per Novelup TOC page.
        if let Some(total) = total_episode_count(site, doc) {
            let last = total.saturating_sub(1) / 100 + 1;
            if last > 10_000 {
                bail!("목차 페이지 수가 지원 범위를 벗어났습니다: {last}");
            }
            pages.extend(2..=last);
        }
    }
    let selector = Selector::parse("a[href]").expect("static selector");
    for anchor in doc.select(&selector) {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(link) = base.join(href) else {
            continue;
        };
        if !matches!(link.scheme(), "http" | "https")
            || link.host_str() != base.host_str()
            || !link.username().is_empty()
            || link.password().is_some()
            || link.port().is_some()
            || link.path().trim_end_matches('/') != base.path().trim_end_matches('/')
        {
            continue;
        }
        let numbers: Vec<_> = link
            .query_pairs()
            .filter(|(key, _)| key == "p")
            .filter_map(|(_, value)| value.parse::<usize>().ok())
            .collect();
        if let [page] = numbers.as_slice() {
            if *page > 10_000 {
                bail!("목차 페이지 수가 지원 범위를 벗어났습니다: {page}");
            }
            if *page > 1 {
                pages.insert(*page);
            }
        }
    }
    if let Some(last) = pages.last().copied() {
        pages.extend(2..=last);
    }
    Ok(pages)
}
/// Additional TOC pages only; the caller already has the canonical first page.
/// Call this on newly fetched pages too, because some pagers expose only "next".
pub fn index_page_urls(url: &str, html: &str) -> Result<Vec<String>> {
    let (site, canonical) = canonicalize(url)?;
    let base = Url::parse(&canonical)?;
    let doc = Html::parse_document(html);
    Ok(index_page_numbers(site, &base, &doc)?
        .into_iter()
        .map(|page| {
            let mut link = base.clone();
            link.set_query(Some(&format!("p={page}")));
            link.to_string()
        })
        .collect())
}
pub fn parse_work(url: &str, html: &str) -> Result<Work> {
    parse_work_pages(url, &[html.to_owned()])
}
/// Pages must be in TOC page order. Only episode links actually present in the
/// supplied documents are used; a total count never creates episode URLs.
pub fn parse_work_pages(url: &str, pages: &[String]) -> Result<Work> {
    let (site, canonical) = canonicalize(url)?;
    let base = Url::parse(&canonical)?;
    let documents: Vec<_> = pages
        .iter()
        .map(|html| Html::parse_document(html))
        .collect();
    let doc = documents.first().context("작품 목차 HTML이 없습니다")?;
    let kakuyomu = (site == Site::Kakuyomu)
        .then(|| kakuyomu_next_data(doc, &base))
        .flatten();
    let alphapolis = if site == Site::Alphapolis {
        alphapolis_cover_data(doc, &base)?
    } else {
        None
    };
    let title_candidates: &[&str] = match site {
        Site::Kakuyomu => &["h1", "meta[property='og:title']"],
        Site::Alphapolis => &[
            ".content-main .title",
            ".novel-title",
            "h1",
            ".title",
            "meta[property='og:title']",
            "meta[name='twitter:title']",
        ],
        Site::Syosetu | Site::Syosetu18 => &[".p-novel__title", "h1", ".novel_title"],
        Site::Hameln => &["[itemprop='name']", "h1"],
        Site::Novelup => &[".storyTitle", "#section_works_info .novel_title", "h1"],
    };
    let title = kakuyomu
        .as_ref()
        .map(|data| data.title.clone())
        .filter(|title| !title.trim().is_empty())
        .or_else(|| alphapolis.as_ref().map(|data| data.title.clone()))
        .or_else(|| first_text(doc, title_candidates))
        .context("작품 제목을 찾지 못했습니다")?;
    let alpha_description = if site == Site::Alphapolis {
        first_element(doc, &[".content-main .abstract", ".abstract"])
            .map(|element| element_lines(element, true).join("\n"))
            .filter(|text| !text.is_empty())
    } else {
        None
    };
    let desc = kakuyomu
        .as_ref()
        .map(|data| data.description.clone())
        .filter(|desc| !desc.trim().is_empty())
        .or(alpha_description)
        .or_else(|| {
            first_text(
                doc,
                &[
                    ".abstract",
                    ".novel_synopsis",
                    ".introduction",
                    ".p-novel__summary",
                    "meta[name='description']",
                ],
            )
        })
        .or_else(|| {
            if site == Site::Hameln {
                let selector = Selector::parse("#maind .ss").ok()?;
                doc.select(&selector)
                    .nth(1)
                    .map(|element| clean(&element.text().collect::<String>()))
            } else {
                None
            }
        })
        .unwrap_or_default();
    let mut episodes = Vec::new();
    let mut seen = HashSet::new();
    let mut advertised = None;
    let mut index_pages = BTreeSet::new();
    let mut unresolved_toc = false;
    let mut redundant_pages = false;
    let mut empty_pages = false;
    for (page_index, document) in documents.iter().enumerate() {
        if let Some(count) = total_episode_count(site, document) {
            if advertised.is_some_and(|previous| previous != count) {
                bail!("목차 전체 화수가 페이지마다 다릅니다. 같은 시점의 목차를 다시 수집하세요");
            }
            advertised = Some(count);
        }
        index_pages.extend(index_page_numbers(site, &base, document)?);
        let mut links = collect_links(site, &base, document);
        if site == Site::Kakuyomu {
            if let Some(data) = kakuyomu_next_data(document, &base) {
                unresolved_toc |= data.unresolved_toc;
                if let Some(count) = data.episode_count {
                    if advertised.is_some_and(|previous| previous != count) {
                        bail!("카쿠요무 목차와 작품 정보의 전체 화수가 일치하지 않습니다");
                    }
                    advertised = Some(count);
                }
                if data.episodes.len() >= links.len() && !data.episodes.is_empty() {
                    links = data.episodes;
                }
            }
        }
        if site == Site::Alphapolis {
            if let Some(data) = alphapolis_cover_data(document, &base)? {
                // JSON access flags remain authoritative even if the DOM also
                // contains rental links or episodes that are not public.
                links = data.episodes;
            }
        }
        empty_pages |= documents.len() > 1 && links.is_empty();
        let before = episodes.len();
        episodes.extend(links.into_iter().filter(|link| seen.insert(link.clone())));
        redundant_pages |= page_index > 0 && episodes.len() == before;
    }
    if episodes.is_empty()
        && matches!(site, Site::Syosetu | Site::Syosetu18)
        && parse_episode(site, &pages[0], false).is_ok()
    {
        // A Syosetu short story is served at the work URL and has no TOC.
        episodes.push(canonical.clone());
    }
    if let Some(count) = advertised {
        if episodes.len() < count {
            bail!("목차가 일부만 로드되었습니다: 수집 {}화 / 사이트 표시 {}화. 추가 목차 페이지를 모두 제공해야 합니다",
                episodes.len(), count);
        }
        if episodes.len() > count {
            bail!(
                "목차 화수와 사이트 표시가 일치하지 않습니다: 수집 {}화 / 사이트 표시 {}화",
                episodes.len(),
                count
            );
        }
    }
    let complete_count = advertised.is_some_and(|count| count == episodes.len());
    if empty_pages || redundant_pages {
        bail!("목차에 빈 페이지 또는 새 에피소드 링크가 없는 중복 페이지가 포함되어 있습니다");
    }
    if !complete_count && index_pages.len() + 1 > documents.len() {
        bail!("페이지 분할 목차가 완전하지 않습니다: 제공 {}페이지 / 확인된 추가 목차 {}페이지. 추가 목차 페이지를 모두 제공해야 합니다",
            documents.len(), index_pages.len());
    }
    if unresolved_toc && !complete_count {
        bail!("카쿠요무 목차 참조를 모두 해석하지 못했습니다. 완전한 목차 HTML이 필요합니다");
    }
    if episodes.is_empty() {
        bail!("목차 링크를 찾지 못했습니다. 인증/연령제한/동적 페이지 여부를 확인하세요");
    }
    Ok(Work {
        site,
        url: canonical,
        title,
        description: desc,
        episodes,
    })
}
fn extract_text(node: NodeRef<'_, Node>, out: &mut String, include_furigana: bool) {
    match node.value() {
        Node::Text(x) => out.push_str(x),
        Node::Element(e) => {
            let tag = e.name();
            if matches!(tag, "script" | "style" | "noscript")
                || (!include_furigana && matches!(tag, "rp" | "rt"))
            {
                return;
            }
            if tag == "br" {
                out.push('\n');
                return;
            }
            for child in node.children() {
                extract_text(child, out, include_furigana);
            }
            if matches!(tag, "p" | "div" | "section" | "li" | "h2" | "h3") {
                out.push('\n');
            }
        }
        _ => {
            for child in node.children() {
                extract_text(child, out, include_furigana);
            }
        }
    }
}
pub fn parse_episode(site: Site, html: &str, include_furigana: bool) -> Result<Episode> {
    parse_episode_with_comments(site, html, include_furigana, false, "")
}
fn first_element<'a>(document: &'a Html, selectors: &[&str]) -> Option<ElementRef<'a>> {
    selectors.iter().find_map(|query| {
        let selector = Selector::parse(query).ok()?;
        document
            .select(&selector)
            .find(|element| !clean(&element.text().collect::<String>()).is_empty())
    })
}
fn element_lines(element: ElementRef<'_>, include_furigana: bool) -> Vec<String> {
    let mut text = String::new();
    extract_text(*element, &mut text, include_furigana);
    let lines: Vec<String> = text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::trim_end)
        .map(str::to_string)
        .collect();
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(start, |last| last + 1);
    lines[start..end].to_vec()
}
fn source_table_text(node: NodeRef<'_, Node>, out: &mut String) {
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
fn alphapolis_source_table(document: &Html, body_selectors: &[&str]) -> Option<Html> {
    let selector = Selector::parse("tbody").ok()?;
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
pub fn parse_episode_with_comments(
    site: Site,
    html: &str,
    include_furigana: bool,
    include_comment: bool,
    comment_separator: &str,
) -> Result<Episode> {
    let direct_document = Html::parse_document(html);
    let (title_selectors,body_selectors):(&[&str],&[&str])=match site {
        Site::Kakuyomu => (&[".widget-episodeTitle", "h1"],&[".widget-episodeBody", "#contentMain .widget-episodeBody", "[data-testid='episode-body']"]),
        Site::Alphapolis => (&[".episode-title", ".novel-episode__title", "h1"],&["#novelBody", ".novel-body", ".novel-body__body", ".episode-body", "[itemprop='articleBody']"]),
        Site::Syosetu|Site::Syosetu18 => (&[".p-novel__title--rensai", ".novel_subtitle", ".p-novel__title", "h1"],
            &["#novel_honbun", ".js-novel-text.p-novel__text:not(.p-novel__text--preface):not(.p-novel__text--afterword)", ".novel_view"]),
        Site::Hameln => (&["h1"],&["#honbun"]),
        Site::Novelup => (&["#js-scroll-area .episode_title", ".episode_title", "h1"],&["#js-scroll-area .content"]),
    };
    let source_document =
        if site == Site::Alphapolis && first_element(&direct_document, body_selectors).is_none() {
            alphapolis_source_table(&direct_document, body_selectors)
        } else {
            None
        };
    let document = source_document.as_ref().unwrap_or(&direct_document);
    let hameln_title = if site == Site::Hameln {
        let selector = Selector::parse("[style]").expect("static selector");
        document
            .select(&selector)
            .filter(|element| {
                element.value().attr("style").is_some_and(|style| {
                    style
                        .split_whitespace()
                        .collect::<String>()
                        .trim_end_matches(';')
                        == "font-size:120%"
                })
            })
            .nth(1)
            .map(|element| clean(&element.text().collect::<String>()))
            .filter(|title| !title.is_empty())
    } else {
        None
    };
    let title = hameln_title
        .or_else(|| first_text(document, title_selectors))
        .unwrap_or_default();
    let body = first_element(document, body_selectors).or_else(|| {
        if !matches!(site, Site::Syosetu | Site::Syosetu18) {
            return None;
        }
        let selector = Selector::parse("#L1").ok()?;
        document
            .select(&selector)
            .next()?
            .parent()
            .and_then(ElementRef::wrap)
    });
    let body = body.context("작품 본문을 찾지 못했습니다(로그인·유료·차단·HTML 변경 가능성)")?;
    let mut lines = element_lines(body, include_furigana);
    if !lines.iter().any(|x| !x.trim().is_empty()) {
        bail!("빈 본문입니다");
    }
    if include_comment {
        let (before, after): (&[&str], &[&str]) = match site {
            Site::Syosetu | Site::Syosetu18 => (
                &[".js-novel-text.p-novel__text--preface", "#novel_p"],
                &[".js-novel-text.p-novel__text--afterword", "#novel_a"],
            ),
            Site::Hameln => (&["#maegaki"], &["#atogaki"]),
            Site::Novelup => (&[".novel_foreword"], &[".novel_afterword"]),
            _ => (&[], &[]),
        };
        if let Some(comment) = first_element(document, before) {
            let mut prefaced = element_lines(comment, include_furigana);
            if !prefaced.is_empty() {
                prefaced.push(comment_separator.to_owned());
                prefaced.extend(lines);
                lines = prefaced;
            }
        }
        if let Some(comment) = first_element(document, after) {
            let comment = element_lines(comment, include_furigana);
            if !comment.is_empty() {
                lines.push(comment_separator.to_owned());
                lines.extend(comment);
            }
        }
    }
    Ok(Episode { title, lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_validation() {
        assert_eq!(
            canonicalize("https://www.alphapolis.co.jp/novel/767156331/284095783")
                .unwrap()
                .1,
            "https://www.alphapolis.co.jp/novel/767156331/284095783/"
        );
        assert_eq!(
            canonicalize("https://kakuyomu.jp/works/2912051603596422650/episodes/123")
                .unwrap()
                .1,
            "https://kakuyomu.jp/works/2912051603596422650"
        );
        assert!(canonicalize("https://kakuyomu.jp.evil.test/works/1").is_err());
        assert!(canonicalize("file:///etc/passwd").is_err());
    }
    #[test]
    fn alphapolis_toc_and_episode() {
        let work=parse_work("https://www.alphapolis.co.jp/novel/767156331/284095783",
            "<div class='content-main'><h1 class='title'>作品名</h1></div><div class='episode'><a href='/novel/767156331/284095783/episode/123'>1話</a></div>").unwrap();
        assert_eq!(work.episodes.len(), 1);
        let episode=parse_episode(Site::Alphapolis,
            "<h1 class='episode-title'>第一話</h1><div id='novelBody'><p>本文<ruby>漢<rt>かん</rt></ruby>字</p><p>次の行</p></div>",false).unwrap();
        assert_eq!(episode.title, "第一話");
        assert!(episode.lines.join(" ").contains("本文漢字"));
        assert!(!episode.lines.join(" ").contains("かん"));
    }
    #[test]
    fn meta_title_and_description() {
        let index = r#"<meta property="og:title" content="テスト作品"><meta name="description" content="紹介文">
          <a href="/novel/767156331/284095783/episode/42">本文へ</a>"#;
        let work = parse_work(
            "https://www.alphapolis.co.jp/novel/767156331/284095783",
            index,
        )
        .unwrap();
        assert_eq!(work.title, "テスト作品");
        assert_eq!(work.description, "紹介文");
    }
    #[test]
    fn preview_only_toc_is_rejected() {
        let url = "https://kakuyomu.jp/works/2912051603596422650";
        let html = r#"<h1>本文</h1><div>連載中 全118話</div>
            <a href="/works/2912051603596422650/episodes/1">1</a>
            <a href="/works/2912051603596422650/episodes/2">2</a>"#;
        assert!(parse_work(url, html)
            .unwrap_err()
            .to_string()
            .contains("수집 2화 / 사이트 표시 118화"));
    }
    #[test]
    fn kakuyomu_toc() {
        let work=parse_work("https://kakuyomu.jp/works/2912051603596422650",
            "<h1>タイトル</h1><a href='/works/2912051603596422650/episodes/1'>A</a><a href='/works/2912051603596422650/episodes/2'>B</a>").unwrap();
        assert_eq!(work.episodes.len(), 2);
    }
    #[test]
    fn syosetu_body_is_separate_from_author_comments() {
        for site in [Site::Syosetu, Site::Syosetu18] {
            for html in [
                "<h1 class='p-novel__title p-novel__title--rensai'>第一話</h1><div class='js-novel-text p-novel__text p-novel__text--preface'><p>前書き</p></div><div class='js-novel-text p-novel__text'><p id='L1'>本<ruby>文<rt>ぶん</rt></ruby></p></div><div class='js-novel-text p-novel__text p-novel__text--afterword'><p>後書き</p></div>",
                "<div class='novel_subtitle'>第一話</div><div id='novel_p'><p>前書き</p></div><div id='novel_honbun'><p>本<ruby>文<rt>ぶん</rt></ruby></p></div><div id='novel_a'><p>後書き</p></div>",
            ] {
                let body = parse_episode(site, html, false).unwrap();
                assert_eq!(body.title, "第一話");
                assert_eq!(body.lines, ["本文"]);
                let comments = parse_episode_with_comments(site, html, false, true, "----").unwrap();
                assert_eq!(comments.lines, ["前書き", "----", "本文", "----", "後書き"]);
                assert!(parse_episode(site, html, true).unwrap().lines.join("").contains("ぶん"));
            }
        }
    }
    #[test]
    fn author_comments_cannot_substitute_for_missing_body() {
        let html = "<h1>第一話</h1><div class='js-novel-text p-novel__text p-novel__text--preface'><p>前書きのみ</p></div>";
        assert!(parse_episode_with_comments(Site::Syosetu, html, false, true, "---").is_err());
    }
    #[test]
    fn hameln_title_summary_and_comments() {
        let index = "<div id='maind'><div class='ss'><span itemprop='name'>作品名</span></div><div class='ss'>作品紹介</div><div class='ss'><a class='episode-list__link' href='./1.html'>第一話</a><a href='./?mode=review'>感想</a></div></div>";
        let work = parse_work("https://syosetu.org/novel/123/", index).unwrap();
        assert_eq!(work.title, "作品名");
        assert_eq!(work.description, "作品紹介");
        assert_eq!(work.episodes, ["https://syosetu.org/novel/123/1.html"]);
        let html = "<div class='ss'><span style='font-size:120%'>作品名</span><span style='font-size: 120%;'>第一話</span><div id='maegaki'>前書き<br>続き</div><div id='honbun'><p>本文</p></div><div id='atogaki'><p>後書き</p></div></div>";
        let episode = parse_episode_with_comments(Site::Hameln, html, false, true, "***").unwrap();
        assert_eq!(episode.title, "第一話");
        assert_eq!(
            episode.lines,
            ["前書き", "続き", "***", "本文", "***", "後書き"]
        );
        assert_eq!(
            parse_episode(Site::Hameln, html, false).unwrap().lines,
            ["本文"]
        );
    }
    #[test]
    fn both_novelup_index_layouts_are_supported() {
        for index in [
            "<h1 class='storyTitle'>作品名</h1><div class='novel_synopsis'>紹介文</div><span class='totalEpisode'>全2エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/11'>第一話</a><a class='episodeTitle' href='/story/123/12'>第二話</a></div>",
            "<div id='section_works_info'><h2 class='novel_title'>作品名</h2></div><span class='total_episode_num'>2</span><div id='section_episode'><div class='episode_link'><a href='/story/123/11'>第一話</a></div><div class='episode_link'><a href='/story/123/12'>第二話</a></div></div>",
        ] {
            let work = parse_work("https://novelup.plus/story/123", index).unwrap();
            assert_eq!(work.title, "作品名");
            assert_eq!(work.episodes, ["https://novelup.plus/story/123/11", "https://novelup.plus/story/123/12"]);
        }
    }
    #[test]
    fn novelup_author_comments_and_ruby() {
        let html = "<div class='novel_foreword'>前書き</div><div id='js-scroll-area'><h2 class='episode_title'>第一話</h2><div class='content'>本文<ruby>字<rp>(</rp><rt>じ</rt><rp>)</rp></ruby><br>次の行</div></div><div class='novel_afterword'>後書き</div>";
        let episode = parse_episode_with_comments(Site::Novelup, html, false, true, "***").unwrap();
        assert_eq!(episode.title, "第一話");
        assert_eq!(
            episode.lines,
            ["前書き", "***", "本文字", "次の行", "***", "後書き"]
        );
    }
    fn novelup_page(start: usize, end: usize) -> String {
        let mut page = "<h1 class='storyTitle'>作品名</h1><span class='totalEpisode'>全201エピソード</span><div class='episodeList'>".to_owned();
        for episode in start..=end {
            page.push_str(&format!(
                "<a class='episodeTitle' href='/story/123/{episode}'>第{episode}話</a>"
            ));
        }
        page.push_str("</div>");
        page
    }
    #[test]
    fn novelup_pagination_requires_real_episode_links_from_all_pages() {
        let first = novelup_page(1, 100);
        assert_eq!(
            index_page_urls("https://novelup.plus/story/123", &first).unwrap(),
            [
                "https://novelup.plus/story/123?p=2",
                "https://novelup.plus/story/123?p=3"
            ]
        );
        assert!(parse_work("https://novelup.plus/story/123", &first)
            .unwrap_err()
            .to_string()
            .contains("수집 100화 / 사이트 표시 201화"));
        assert!(parse_work_pages(
            "https://novelup.plus/story/123",
            &[first.clone(), novelup_page(101, 200)]
        )
        .is_err());
        let work = parse_work_pages(
            "https://novelup.plus/story/123",
            &[first, novelup_page(101, 200), novelup_page(201, 201)],
        )
        .unwrap();
        assert_eq!(work.episodes.len(), 201);
        assert_eq!(work.episodes[100], "https://novelup.plus/story/123/101");
        assert_eq!(work.episodes[200], "https://novelup.plus/story/123/201");
    }
    #[test]
    fn pagination_only_uses_same_work_and_does_not_create_episodes() {
        let first = "<h1>作品名</h1><a href='/n1234ab/1/'>第一話</a><a href='?p=3'>最後へ</a><a href='https://evil.test/n1234ab/?p=8'>外部</a><a href='/n9999zz/?p=10'>別作品</a><a href='/n1234ab/review/'>感想</a><a href='/n1234ab/1/?tracking=1#top'>第一話再掲</a>".to_owned();
        let url = "https://ncode.syosetu.com/n1234ab/";
        assert_eq!(
            index_page_urls(url, &first).unwrap(),
            [
                "https://ncode.syosetu.com/n1234ab/?p=2",
                "https://ncode.syosetu.com/n1234ab/?p=3"
            ]
        );
        assert!(parse_work(url, &first).is_err());
        let work = parse_work_pages(
            url,
            &[
                first,
                "<a href='/n1234ab/2/'>第二話</a>".to_owned(),
                "<a href='/n1234ab/3/'>第三話</a>".to_owned(),
            ],
        )
        .unwrap();
        assert_eq!(
            work.episodes,
            [
                "https://ncode.syosetu.com/n1234ab/1/",
                "https://ncode.syosetu.com/n1234ab/2/",
                "https://ncode.syosetu.com/n1234ab/3/"
            ]
        );
    }
    #[test]
    fn repeated_toc_pages_do_not_hide_missing_episodes() {
        let first =
            "<h1>作品名</h1><a href='/n1234ab/1/'>第一話</a><a href='?p=2'>次へ</a>".to_owned();
        assert!(parse_work_pages(
            "https://ncode.syosetu.com/n1234ab/",
            &[first.clone(), first]
        )
        .is_err());
    }
    #[test]
    fn empty_or_repeated_toc_pages_are_rejected_even_when_count_matches() {
        let complete = "<h1 class='storyTitle'>作品名</h1><span class='totalEpisode'>全1エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/11'>第一話</a></div>".to_owned();
        assert!(parse_work_pages(
            "https://novelup.plus/story/123",
            &[complete.clone(), complete.clone()]
        )
        .is_err());
        assert!(
            parse_work_pages("https://novelup.plus/story/123", &[complete, String::new()]).is_err()
        );
        assert!(parse_work_pages("https://novelup.plus/story/123", &[]).is_err());
    }
    #[test]
    fn syosetu_short_story_and_l1_body() {
        let url = "https://ncode.syosetu.com/N1234AB/";
        let html = "<h1 class='p-novel__title'>短編作品</h1><section><p id='L1'>a</p></section>";
        let work = parse_work(url, html).unwrap();
        assert_eq!(work.episodes, ["https://ncode.syosetu.com/n1234ab/"]);
        assert_eq!(
            parse_episode(Site::Syosetu, html, false).unwrap().lines,
            ["a"]
        );
        assert!(parse_work(url, "<h1>作品名</h1><a href='?p=2'>次へ</a>").is_err());
    }
    fn kakuyomu_json_page(state: Value) -> String {
        let data = serde_json::json!({"props":{"pageProps":{"__APOLLO_STATE__":state}}});
        format!("<script id='__NEXT_DATA__' type='application/json'>{data}</script>")
    }
    #[test]
    fn kakuyomu_resolves_ordered_toc_and_ignores_chapter_and_other_work_ids() {
        let html = kakuyomu_json_page(serde_json::json!({
            "Work:123": {"title":"作品名", "introduction":"全999話を予定\n二行目", "tableOfContents":[{"__ref":"TOC:1"}]},
            "Work:1234": {"title":"別作品", "introduction":"別紹介文"},
            "TOC:1": {"chapter":{"__ref":"Chapter:999"}, "episodeUnions":[{"__ref":"Episode:20"},{"__ref":"Episode:3"},{"__ref":"Episode:20"}]},
            "Chapter:999": {"id":"999", "__typename":"Chapter"},
            "Episode:20": {"id":"20"}, "Episode:3": {"id":"3"}, "Episode:10": {"id":"10"}
        }));
        let work = parse_work("https://kakuyomu.jp/works/123", &html).unwrap();
        assert_eq!(work.title, "作品名");
        assert_eq!(work.description, "全999話を予定\n二行目");
        assert_eq!(
            work.episodes,
            [
                "https://kakuyomu.jp/works/123/episodes/20",
                "https://kakuyomu.jp/works/123/episodes/3"
            ]
        );
    }
    #[test]
    fn unresolved_kakuyomu_toc_and_unscoped_episode_cache_are_rejected() {
        for info in [
            serde_json::json!({"title":"作品名", "tableOfContents":[{"__ref":"Missing:1"},{"__ref":"Episode:20"}]}),
            serde_json::json!({"title":"作品名"}),
        ] {
            let html =
                kakuyomu_json_page(serde_json::json!({"Work:123":info,"Episode:20":{"id":"20"}}));
            assert!(parse_work("https://kakuyomu.jp/works/123", &html).is_err());
        }
    }
    #[test]
    fn conflicting_index_counts_and_unknown_episode_unions_are_rejected() {
        let first = "<h1 class='storyTitle'>作品名</h1><span class='totalEpisode'>全2エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/11'>第一話</a></div>".to_owned();
        let second = "<span class='totalEpisode'>全3エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/12'>第二話</a></div>".to_owned();
        assert!(
            parse_work_pages("https://novelup.plus/story/123", &[first, second])
                .unwrap_err()
                .to_string()
                .contains("페이지마다 다릅니다")
        );
        let html = kakuyomu_json_page(serde_json::json!({
            "Work:123": {"title":"作品名", "tableOfContents":[{"episodeUnions":[
                {"__ref":"Episode:1"}, {"id":"2","__typename":"UnknownEpisode"}
            ]}]}
        }));
        assert!(parse_work("https://kakuyomu.jp/works/123", &html).is_err());
    }
    fn kakuyomu_v2_state() -> Value {
        serde_json::json!({
            "Work:123": {"__typename":"Work", "id":"123", "title":"合成作品", "introduction":"合成紹介文", "publicEpisodeCount":3,
                "tableOfContentsV2":[{"__ref":"TableOfContentsChapter:2"},{"__ref":"TableOfContentsChapter:1"}],
                "tableOfContents":[{"__ref":"Episode:4"}]},
            "TableOfContentsChapter:2": {"__typename":"TableOfContentsChapter", "id":"2", "chapter":{"__ref":"Chapter:999"},
                "episodeUnions":[{"__ref":"Episode:20"},{"__ref":"Episode:3"}]},
            "TableOfContentsChapter:1": {"__typename":"TableOfContentsChapter", "id":"1", "chapter":null,
                "episodeUnions":[{"__ref":"Episode:1"}]},
            "Episode:20":{"__typename":"Episode", "id":"20"}, "Episode:3":{"__typename":"Episode", "id":"3"},
            "Episode:1":{"__typename":"Episode", "id":"1"}, "Episode:4":{"__typename":"Episode", "id":"4"},
            "Chapter:999":{"__typename":"Chapter", "id":"999"}
        })
    }
    #[test]
    fn kakuyomu_v2_uses_defined_episode_unions_in_chapter_and_episode_order() {
        let html = format!(
            "{}<a href='/works/123/episodes/20'>合成部分目次</a>",
            kakuyomu_json_page(kakuyomu_v2_state())
        );
        let work = parse_work("https://kakuyomu.jp/works/123", &html).unwrap();
        assert_eq!(work.title, "合成作品");
        assert_eq!(
            work.episodes,
            [
                "https://kakuyomu.jp/works/123/episodes/20",
                "https://kakuyomu.jp/works/123/episodes/3",
                "https://kakuyomu.jp/works/123/episodes/1"
            ]
        );
    }
    #[test]
    fn kakuyomu_v2_rejects_missing_or_mismatched_nodes_and_unknown_unions() {
        for change in 0..5 {
            let mut state = kakuyomu_v2_state();
            match change {
                0 => {
                    state.as_object_mut().unwrap().remove("Episode:3");
                }
                1 => state["Episode:3"]["id"] = serde_json::json!("999"),
                2 => state["Episode:3"]["__typename"] = serde_json::json!("UnknownEpisode"),
                3 => state["TableOfContentsChapter:2"]["__typename"] = serde_json::json!("Chapter"),
                _ => state["TableOfContentsChapter:2"]["episodeUnions"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .map(|_| ())
                    .unwrap(),
            }
            let html = kakuyomu_json_page(state);
            assert!(
                parse_work("https://kakuyomu.jp/works/123", &html).is_err(),
                "invalid V2 case {change}"
            );
        }
    }
    fn alphapolis_json_page(chapters: Value) -> String {
        alphapolis_cover_page(serde_json::json!({
            "content":{"id":2, "url":"/novel/1/2/", "title":"合成作品", "isNovel":true},
            "chapterEpisodes":chapters, "isPartialEpisode":false, "isPreview":false
        }))
    }
    fn alphapolis_cover_page(data: Value) -> String {
        let json = data.to_string().replace('/', "\\/");
        format!("<div id='app-cover-episode'></div><script type='application/json' id='app-cover-data'>{json}</script>")
    }
    fn alphapolis_public_chapter(episodes: Value) -> Value {
        serde_json::json!({"isRental":false, "rentalBookId":null, "book":null, "episodes":episodes, "extraEpisode":null})
    }
    fn alphapolis_episode(id: u64, order: u64, public: bool, rental: Value) -> Value {
        serde_json::json!({"episodeNo":id, "url":format!("/novel/1/2/episode/{id}"), "dispOrder":order, "isPublic":public, "rental":rental})
    }
    #[test]
    fn alphapolis_cover_json_only_reads_explicitly_public_non_rental_episodes() {
        let free = alphapolis_public_chapter(serde_json::json!([
            alphapolis_episode(20,2,true,Value::Null), alphapolis_episode(10,1,true,Value::Null),
            alphapolis_episode(30,3,false,Value::Null), alphapolis_episode(40,4,true,serde_json::json!({"id":4})),
            {"episodeNo":80,"url":"/novel/1/2/episode/80","dispOrder":8,"rental":null}
        ]));
        let mut rental = alphapolis_public_chapter(serde_json::json!([alphapolis_episode(
            50,
            5,
            true,
            Value::Null
        )]));
        rental["isRental"] = serde_json::json!(true);
        let mut rental_book = alphapolis_public_chapter(serde_json::json!([alphapolis_episode(
            60,
            6,
            true,
            Value::Null
        )]));
        rental_book["rentalBookId"] = serde_json::json!(6);
        let mut book = alphapolis_public_chapter(serde_json::json!([alphapolis_episode(
            70,
            7,
            true,
            Value::Null
        )]));
        book["book"] = serde_json::json!({"id":7});
        let html = format!("{}<div class='episode'><a href='/novel/1/2/episode/30'>非公開</a><a href='/novel/1/2/episode/40'>レンタル</a><a href='/novel/1/2/episode/50'>書籍</a></div>", alphapolis_json_page(serde_json::json!([free,rental,rental_book,book])));
        assert!(html.contains(r"\/novel\/1\/2\/episode\/20"));
        let work = parse_work("https://www.alphapolis.co.jp/novel/1/2/", &html).unwrap();
        assert_eq!(work.title, "合成作品");
        assert_eq!(
            work.episodes,
            [
                "https://www.alphapolis.co.jp/novel/1/2/episode/10",
                "https://www.alphapolis.co.jp/novel/1/2/episode/20"
            ]
        );
    }
    #[test]
    fn alphapolis_cover_json_does_not_fall_back_to_rental_dom_links() {
        let chapter = alphapolis_public_chapter(serde_json::json!([
            alphapolis_episode(10, 1, false, Value::Null),
            alphapolis_episode(20, 2, true, serde_json::json!({"id":2}))
        ]));
        let html = format!(
            "{}<h1>合成作品</h1><a href='/novel/1/2/episode/20'>レンタル</a>",
            alphapolis_json_page(serde_json::json!([chapter]))
        );
        assert!(parse_work("https://www.alphapolis.co.jp/novel/1/2/", &html).is_err());
    }
    #[test]
    fn alphapolis_cover_json_rejects_partial_mismatched_and_ambiguous_data() {
        for change in 0..5 {
            let mut data = serde_json::json!({
                "content":{"id":2,"url":"/novel/1/2/","title":"合成作品","isNovel":true},
                "chapterEpisodes":[alphapolis_public_chapter(serde_json::json!([alphapolis_episode(10,1,true,Value::Null)]))],
                "isPartialEpisode":false,"isPreview":false
            });
            match change {
                0 => data["isPartialEpisode"] = serde_json::json!(true),
                1 => data["content"]["url"] = serde_json::json!("/novel/1/999/"),
                2 => {
                    data["chapterEpisodes"][0]["episodes"][0]["url"] =
                        serde_json::json!("/novel/1/2/episode/999")
                }
                3 => {
                    let duplicate = data["chapterEpisodes"][0]["episodes"][0].clone();
                    data["chapterEpisodes"][0]["episodes"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }
                _ => {
                    data["chapterEpisodes"][0]["extraEpisode"] =
                        serde_json::json!({"url":"/novel/1/2/episode/30"})
                }
            }
            assert!(
                parse_work(
                    "https://www.alphapolis.co.jp/novel/1/2/",
                    &alphapolis_cover_page(data)
                )
                .is_err(),
                "invalid cover case {change}"
            );
        }
    }
    #[test]
    fn alphapolis_saved_source_table_and_multiline_summary() {
        let source = "<table><tbody><tr><td class='line-number'>1</td><td class='line-content'>&lt;h1 class='episode-title'&gt;第一話&lt;/h1&gt;</td></tr><tr><td class='line-number'>2</td><td class='line-content'>&lt;div id='novelBody'&gt;&lt;p&gt;本文&lt;/p&gt;&lt;/div&gt;</td></tr></tbody></table>";
        let episode = parse_episode(Site::Alphapolis, source, false).unwrap();
        assert_eq!(episode.title, "第一話");
        assert_eq!(episode.lines, ["本文"]);
        let work = parse_work("https://www.alphapolis.co.jp/novel/1/2", "<h1>作品名</h1><div class='abstract'>一行目<br>二行目</div><a href='https://alphapolis.co.jp/novel/1/2/episode/3'>第一話</a>").unwrap();
        assert_eq!(work.description, "一行目\n二行目");
        assert_eq!(
            work.episodes,
            ["https://www.alphapolis.co.jp/novel/1/2/episode/3"]
        );
    }
}
