//! Independently authored dialogue defaults, migrated once with user precedence.
use super::{Memory, MAX_ENTRIES};
use anyhow::Result;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    version: u32,
    entries: BTreeMap<String, String>,
}
fn entries() -> Result<&'static BTreeMap<String, String>> {
    static ENTRIES: OnceLock<std::result::Result<BTreeMap<String, String>, String>> =
        OnceLock::new();
    ENTRIES
        .get_or_init(|| {
            (|| -> Result<_> {
                let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
                    include_bytes!("../../data/learning/ja-ko-speech.json.zlib"),
                    256 * 1024,
                )
                .map_err(|error| anyhow::anyhow!("대화 기본값 압축 해제 실패: {error:?}"))?;
                let asset: Asset = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    asset.version == 1 && asset.entries.len() <= 2000,
                    "대화 기본값 형식 오류"
                );
                for (source, target) in &asset.entries {
                    super::valid(source)?;
                    super::valid(target)?;
                }
                Ok(asset.entries)
            })()
            .map_err(|error| format!("{error:#}"))
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!(error.clone()))
}
pub(super) fn apply(memory: &mut Memory) -> Result<()> {
    if memory.speech_revision >= 1 {
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
    memory.speech_revision = 1;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_short_forms_and_registers_resolve_without_overwriting_user_edits() {
        let mut memory = Memory::bundled().unwrap();
        assert_eq!(entries().unwrap().len(), 112);
        assert_eq!(
            memory.resolve("「行かなきゃ。」").as_deref(),
            Some("“가야 해.”")
        );
        assert_eq!(memory.resolve("「マジで?」").as_deref(), Some("“진짜로?”"));
        assert_eq!(memory.resolve("「りょ」").as_deref(), Some("“알았어.”"));
        assert_eq!(memory.resolve("あざす").as_deref(), Some("고마워요."));
        assert_eq!(
            memory.resolve("『大丈夫ですか？』").as_deref(),
            Some("‘괜찮으세요?’")
        );
        memory
            .learn("「行かなきゃ。」", "이제 출발해야겠어.")
            .unwrap();
        assert_eq!(
            memory.resolve("「行かなきゃ。」").as_deref(),
            Some("이제 출발해야겠어.")
        );
        assert!(Memory::default().resolve("「行かなきゃ。」").is_none());
        memory.learn("了解。", "“알겠어.”").unwrap();
        assert_eq!(memory.resolve("「了解」").as_deref(), Some("“알겠어.”"));
    }
    #[test]
    fn migration_preserves_edits_and_persisted_deletions() {
        let mut old = Memory {
            defaults_applied: true,
            ..Memory::default()
        };
        old.learn("ありがとう！", "정말 고마워!").unwrap();
        let mut migrated = super::super::bootstrap::merge(old).unwrap();
        assert_eq!(migrated.lookup("ありがとう！"), Some("정말 고마워!"));
        migrated.sentences.remove("行かなきゃ。");
        let saved = serde_json::to_vec(&migrated).unwrap();
        let restored = serde_json::from_slice(&saved).unwrap();
        assert!(super::super::bootstrap::merge(restored)
            .unwrap()
            .resolve("「行かなきゃ。」")
            .is_none());
    }
    #[test]
    fn compressed_speech_entries_match_the_reviewable_source() {
        let expected = include_str!("../../data/learning/ja-ko-speech.tsv")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let (ja, ko) = line.split_once('\t').unwrap();
                (ja.to_owned(), ko.to_owned())
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(&expected, entries().unwrap());
    }
}
