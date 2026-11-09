//! Bounded, embedded canonical headword glosses, applied to exact sources only.
use super::{Memory, MAX_ENTRIES};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};
const REVISION: u32 = 1;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    version: u32,
    application: String,
    entries: BTreeMap<String, String>,
}
pub(super) fn entries() -> Result<&'static BTreeMap<String, String>> {
    static ENTRIES: OnceLock<std::result::Result<BTreeMap<String, String>, String>> =
        OnceLock::new();
    let data = ENTRIES.get_or_init(|| {
        (|| -> Result<_> {
            let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
                include_bytes!("../../data/dictionary/ja-ko-core.json.zlib"),
                256 * 1024,
            )
            .map_err(|e| anyhow::anyhow!("내장 사전 압축 해제 실패: {e:?}"))?;
            let asset: Asset = serde_json::from_slice(&bytes).context("내장 사전 해석 실패")?;
            anyhow::ensure!(
                asset.version == 1 && asset.application == "exact" && asset.entries.len() <= 5000,
                "내장 사전 형식 오류"
            );
            for (source, target) in &asset.entries {
                super::valid(source)?;
                super::valid(target)?;
            }
            Ok(asset.entries)
        })()
        .map_err(|e| format!("{e:#}"))
    });
    data.as_ref()
        .map_err(|message| anyhow::anyhow!(message.clone()))
}
pub(super) fn apply(memory: &mut Memory) -> Result<()> {
    if memory.dictionary_revision >= REVISION {
        return Ok(());
    }
    for (source, target) in entries()? {
        if memory.sentences.len() + memory.terms.len() >= MAX_ENTRIES {
            break;
        }
        if memory
            .terms
            .iter()
            .any(|term| source.contains(&term.source))
        {
            continue;
        }
        memory
            .sentences
            .entry(source.clone())
            .or_insert_with(|| target.clone());
    }
    memory.dictionary_revision = REVISION;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_glossary_matches_reviewable_tsv_and_exact_scope() {
        let mut expected = BTreeMap::new();
        for line in include_str!("../../data/dictionary/ja-ko-core.tsv")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let (ja, ko) = line.split_once('\t').unwrap();
            assert!(expected.insert(ja.to_owned(), ko.to_owned()).is_none());
        }
        assert_eq!(&expected, entries().unwrap());
        assert_eq!(expected.len(), 1058);
        let mut memory = Memory::default();
        apply(&mut memory).unwrap();
        assert_eq!(memory.lookup("図書館"), Some("도서관"));
        assert_eq!(memory.lookup("孫"), Some("손주"));
        assert!(memory.lookup("彼女は図書館へ行った。").is_none());
    }
    #[test]
    fn old_profiles_keep_user_corrections_and_deletions_after_dictionary_migration() {
        let mut user = Memory {
            defaults_applied: true,
            ..Memory::default()
        };
        user.learn("犬", "우리 집 강아지").unwrap();
        let mut migrated = super::super::bootstrap::merge(user).unwrap();
        assert_eq!(migrated.lookup("犬"), Some("우리 집 강아지"));
        assert!(migrated.lookup("気にしないでください。").is_none());
        migrated.sentences.remove("図書館");
        let saved = serde_json::to_vec(&migrated).unwrap();
        let reloaded = serde_json::from_slice(&saved).unwrap();
        assert!(super::super::bootstrap::merge(reloaded)
            .unwrap()
            .lookup("図書館")
            .is_none());
    }
}
