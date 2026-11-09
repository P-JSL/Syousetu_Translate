use super::dom::{clean, element_lines, extract_text, first_element, first_text, select};
use super::{
    alphapolis::alphapolis_cover_data, episode::parse_episode, kakuyomu::kakuyomu_next_data,
    links::collect_links, urls::canonicalize,
};
use super::{Site, Work};
use anyhow::{bail, Context, Result};
use scraper::Html;
use std::collections::{BTreeSet, HashSet};
use url::Url;
/// Some public pages serve only a preview of the TOC. Never silently
/// present such a partial index as a complete work.
fn advertised_episode_count(document: &Html) -> Option<usize> {
    let mut text = String::new();
    extract_text(*document.root_element(), &mut text, false);
    let count = regex::Regex::new(r"全\s*([0-9,]+)\s*(?:話|エピソード)").ok()?;
    let raw = count.captures(&text)?.get(1)?.as_str().replace(',', "");
    raw.parse().ok()
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
    let selector = select("a[href]").expect("static selector");
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
    if !matches!(site, Site::Novelup | Site::Syosetu | Site::Syosetu18) {
        return Ok(Vec::new());
    }
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
    parse_work_slices(url, &[html])
}
/// Pages must be in TOC page order. Only episode links actually present in the
/// supplied documents are used; a total count never creates episode URLs.
pub fn parse_work_pages(url: &str, pages: &[String]) -> Result<Work> {
    parse_work_slices(url, &pages.iter().map(String::as_str).collect::<Vec<_>>())
}
fn parse_work_slices(url: &str, pages: &[&str]) -> Result<Work> {
    let (site, canonical) = canonicalize(url)?;
    let base = Url::parse(&canonical)?;
    let documents: Vec<_> = pages
        .iter()
        .map(|html| Html::parse_document(html))
        .collect();
    let doc = documents.first().context("작품 목차 HTML이 없습니다")?;
    let mut kakuyomu = (site == Site::Kakuyomu)
        .then(|| kakuyomu_next_data(doc, &base))
        .flatten();
    let mut alphapolis = if site == Site::Alphapolis {
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
                let selector = select("#maind .ss").ok()?;
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
            if let Some(data) = if page_index == 0 {
                kakuyomu.take()
            } else {
                kakuyomu_next_data(document, &base)
            } {
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
            if let Some(data) = if page_index == 0 {
                alphapolis.take()
            } else {
                alphapolis_cover_data(document, &base)?
            } {
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
        && parse_episode(site, pages[0], false).is_ok()
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
