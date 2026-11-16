//! Restore an explicit standalone address only when its Korean title is absent.
//! These are project-authored Japanese→Korean conditions, not legacy VM rules.
use super::Utterance;
const TITLES: &[(&str, &str)] = &[("陛下", "폐하"), ("殿下", "전하"), ("閣下", "각하")];

pub(super) fn restore(utterance: Utterance<'_>, generated: &str) -> Option<String> {
    if !matches!(utterance.quotes, Some(('“', '”') | ('‘', '’'))) {
        return None;
    }
    let (head, source_body) = utterance.inner.split_once(['、', ',', '，'])?;
    let (_, korean) = TITLES
        .iter()
        .find(|(japanese, _)| head.trim() == *japanese)?;
    if TITLES.iter().any(|(japanese, target)| {
        generated.contains(target)
            || source_body
                .trim_start()
                .starts_with(&format!("{japanese}、"))
    }) {
        return None;
    }
    let (prefix, body) = generated.split_once([',', '、', '，'])?;
    if prefix.trim().is_empty()
        || prefix.chars().count() > 16
        || prefix.contains(['.', '!', '?', '。', '！', '？', ';', ':', '\n'])
        || !body.chars().any(|ch| ('가'..='힣').contains(&ch))
    {
        return None;
    }
    Some(format!("{korean}, {}", body.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses_use_explicit_source_titles_without_inferring_rank_or_speaker() {
        for (source, target) in [
            ("「陛下、こちらへ。」", "폐하"),
            ("『殿下、こちらへ。』", "전하"),
            ("「閣下、こちらへ。」", "각하"),
        ] {
            assert_eq!(
                restore(Utterance::parse(source), "존귀하신 분, 이쪽으로 오십시오.").unwrap(),
                format!("{target}, 이쪽으로 오십시오.")
            );
        }
        for (source, output) in [
            ("陛下、こちらへ。", "존귀하신 분, 이쪽으로 오십시오."),
            ("（陛下、こちらへ。）", "존귀하신 분, 이쪽으로 오십시오."),
            (
                "「偽の陛下、こちらへ。」",
                "존귀하신 분, 이쪽으로 오십시오.",
            ),
            ("「陛下、こちらへ。」", "이쪽으로 오십시오, 폐하."),
            (
                "「陛下、殿下、こちらへ。」",
                "존귀하신 분, 이쪽으로 오십시오.",
            ),
            ("「陛下、こちらへ。」", "폐하께서는, 이쪽으로 오십시오."),
            ("「陛下、こちらへ。」", "이쪽으로 오십시오."),
            (
                "「陛下、こちらへ。」",
                "이미 끝났습니다. 그런데, 이쪽으로 오십시오.",
            ),
        ] {
            assert!(restore(Utterance::parse(source), output).is_none());
        }
    }
}
