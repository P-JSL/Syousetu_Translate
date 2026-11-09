use super::alphapolis::alphapolis_source_table;
use super::dom::{clean, element_lines, first_element, first_text, select};
use super::{Episode, Site};
use anyhow::{bail, Context, Result};
use scraper::{ElementRef, Html};
pub fn parse_episode(site: Site, html: &str, include_furigana: bool) -> Result<Episode> {
    parse_episode_with_comments(site, html, include_furigana, false, "")
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
        let selector = select("[style]").expect("static selector");
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
        let selector = select("#L1").ok()?;
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
