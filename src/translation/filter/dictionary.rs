use super::text::{hiragana_to_hangul, is_japanese};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use anyhow::{Context, Result};
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone)]
pub struct DictionaryEntry {
    pub source: String,
    pub target: String,
    pub name: bool,
}
#[derive(Debug, Clone)]
pub struct Dictionary {
    pub(super) entries: Vec<DictionaryEntry>,
    pub(super) matcher: Option<AhoCorasick>,
}
impl Dictionary {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            matcher: None,
        }
    }
    pub fn parse(text: &str) -> Result<Self> {
        let mut entries = Vec::<DictionaryEntry>::new();
        let mut indices = HashMap::new();
        for line in text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(|x| x.trim_end_matches('\r'))
        {
            if line.trim().is_empty() || line.trim_start().starts_with("//") {
                continue;
            }
            let mut parts = line.split('\t');
            let raw_source = parts.next().unwrap_or("");
            let source = raw_source.trim_end_matches("#N").to_owned();
            if source.is_empty() {
                continue;
            }
            let raw_target = parts.next().unwrap_or("");
            let (target, name) = if raw_target.is_empty() {
                let converted = hiragana_to_hangul(&source);
                if is_japanese(&converted) {
                    continue;
                }
                (converted, raw_source.ends_with("#N"))
            } else {
                (
                    raw_target.trim_end_matches("#N").to_owned(),
                    raw_source.ends_with("#N") || raw_target.ends_with("#N"),
                )
            };
            if target.is_empty() {
                continue;
            }
            let entry = DictionaryEntry {
                source: source.clone(),
                target,
                name,
            };
            if let Some(&idx) = indices.get(&source) {
                entries[idx] = entry;
            } else {
                indices.insert(source, entries.len());
                entries.push(entry);
            }
        }
        entries.sort_by(|a, b| a.source.cmp(&b.source));
        let matcher = if entries.is_empty() {
            None
        } else {
            Some(
                AhoCorasickBuilder::new()
                    .match_kind(MatchKind::LeftmostLongest)
                    .build(entries.iter().map(|e| e.source.as_str()))?,
            )
        };
        Ok(Self { entries, matcher })
    }
    pub fn from_file(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("사전 파일: {}", path.display()))?;
        let string = if bytes.starts_with(&[0xFF, 0xFE]) {
            let (cow, _, errors) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
            if errors {
                anyhow::bail!("UTF-16LE 사전 디코딩 오류");
            }
            cow.into_owned()
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            let (cow, _, errors) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
            if errors {
                anyhow::bail!("UTF-16BE 사전 디코딩 오류");
            }
            cow.into_owned()
        } else {
            String::from_utf8(bytes).context("UTF-8 사전 디코딩 오류")?
        };
        Self::parse(&string)
    }
}
