//! Site adapters with shared DOM extraction and strict TOC completeness checks.
mod alphapolis;
mod dom;
mod episode;
mod index;
mod kakuyomu;
mod links;
mod urls;
use serde::{Deserialize, Serialize};
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

pub use episode::{parse_episode, parse_episode_with_comments};
pub use index::{index_page_urls, parse_work, parse_work_pages};
pub use urls::canonicalize;
#[cfg(test)]
mod tests;
