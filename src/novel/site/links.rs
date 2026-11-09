use super::dom::select;
use super::urls::is_valid_episode;
use super::Site;
use scraper::Html;
use std::collections::HashSet;
use url::Url;
pub(super) fn collect_links(site: Site, base: &Url, html: &Html) -> Vec<String> {
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
        let Ok(selector) = select(selector) else {
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
