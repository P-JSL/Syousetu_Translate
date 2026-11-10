use super::{Memory, MAX_BYTES, MAX_ENTRIES};
use anyhow::{Context, Result};
use std::sync::OnceLock;
pub(super) fn bundled() -> Result<Memory> {
    static MEMORY: OnceLock<std::result::Result<Memory, String>> = OnceLock::new();
    MEMORY
        .get_or_init(|| {
            let result = (|| -> Result<Memory> {
                let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
                    include_bytes!("../../data/learning/ja-ko-natural.json.zlib"),
                    MAX_BYTES as usize,
                )
                .map_err(|e| anyhow::anyhow!("기본 교정 압축 해제 실패: {e:?}"))?;
                let mut memory: Memory =
                    serde_json::from_slice(&bytes).context("기본 교정 데이터 해석 실패")?;
                anyhow::ensure!(
                    memory.version == 1
                        && memory.sentences.len() + memory.terms.len() <= MAX_ENTRIES,
                    "기본 교정 데이터 한도 오류"
                );
                for (source, target) in &memory.sentences {
                    super::valid(source)?;
                    super::valid(target)?;
                }
                for term in &memory.terms {
                    super::valid(&term.source)?;
                    super::valid(&term.previous)?;
                    super::valid(&term.corrected)?;
                }
                super::core_dictionary::apply(&mut memory)?;
                super::speech::apply(&mut memory)?;
                Ok(memory)
            })();
            result.map_err(|e| format!("{e:#}"))
        })
        .clone()
        .map_err(anyhow::Error::msg)
}
pub(super) fn merge(mut user: Memory) -> Result<Memory> {
    if user.defaults_applied {
        super::core_dictionary::apply(&mut user)?;
        super::speech::apply(&mut user)?;
        return Ok(user);
    }
    let defaults = bundled()?;
    for (source, target) in defaults.sentences {
        if user.sentences.len() + user.terms.len() >= MAX_ENTRIES {
            break;
        }
        if user.terms.iter().any(|term| source.contains(&term.source)) {
            continue;
        }
        user.sentences.entry(source).or_insert(target);
    }
    for term in defaults.terms {
        if user.sentences.len() + user.terms.len() >= MAX_ENTRIES {
            break;
        }
        if !user
            .terms
            .iter()
            .any(|existing| existing.source == term.source && existing.previous == term.previous)
        {
            user.terms.push(term);
        }
    }
    user.defaults_applied = true;
    super::core_dictionary::apply(&mut user)?;
    super::speech::apply(&mut user)?;
    Ok(user)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_defaults_match_reviewable_source_and_user_edits_win() {
        let expected: Memory =
            serde_json::from_str(include_str!("../../data/learning/ja-ko-natural.json")).unwrap();
        let defaults = bundled().unwrap();
        let packed = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
            include_bytes!("../../data/learning/ja-ko-natural.json.zlib"),
            MAX_BYTES as usize,
        )
        .unwrap();
        let unpacked: Memory = serde_json::from_slice(&packed).unwrap();
        assert_eq!(
            serde_json::to_value(&unpacked).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        for (source, target) in &expected.sentences {
            assert_eq!(defaults.lookup(source), Some(target.as_str()));
        }
        assert_eq!(
            defaults
                .post_edit("ワーグナーが扉を開けた。", "워그너가 문을 열었다.")
                .unwrap(),
            "바그너가 문을 열었다."
        );
        assert_eq!(
            defaults
                .post_edit("別人が扉を開けた。", "워그너가 문을 열었다.")
                .unwrap(),
            "워그너가 문을 열었다."
        );
        let mut user = Memory::default();
        user.learn("ありがとうございます。", "고맙습니다.").unwrap();
        let mut merged = merge(user).unwrap();
        assert_eq!(merged.lookup("ありがとうございます。"), Some("고맙습니다."));
        merged.sentences.remove("気にしないでください。");
        assert!(merge(merged)
            .unwrap()
            .lookup("気にしないでください。")
            .is_none());
    }
    #[test]
    fn existing_user_terminology_wins_over_new_defaults() {
        let mut user = Memory::default();
        user.learn_term("ワーグナー", "워그너", "와그너").unwrap();
        let memory = merge(user).unwrap();
        assert!(memory.lookup("ワーグナー").is_none());
        assert_eq!(
            memory
                .post_edit("ワーグナーが来た", "워그너가 왔다")
                .unwrap(),
            "와그너가 왔다"
        );
    }
}
