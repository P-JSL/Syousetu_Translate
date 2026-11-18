//! Conservative RPG headwords inside aligned job brackets, not global rewrites.
//! Unknown names, lost brackets and ambiguous mixed lists remain untouched.
use std::{borrow::Cow, ops::Range};

const TERMS: &[(&str, &str)] = &[
    ("勇者", "용사"),
    ("剣士", "검사"),
    ("戦士", "전사"),
    ("拳闘士", "권투사"),
    ("筋肉戦士", "근육 전사"),
    ("大拳闘士", "대권투사"),
    ("錬金術師", "연금술사"),
    ("鍛冶師", "대장장이"),
];

struct Span<'a> {
    text: &'a str,
    inner: Range<usize>,
    end: usize,
}

fn spans(text: &str) -> Option<Vec<Span<'_>>> {
    let mut output = Vec::new();
    let mut open = None;
    for (index, ch) in text.char_indices() {
        match ch {
            '【' | '[' => {
                if open.is_some() {
                    return None;
                }
                open = Some((index + ch.len_utf8(), if ch == '【' { '】' } else { ']' }));
            }
            '】' | ']' => {
                let (start, close) = open.take()?;
                if close != ch || output.len() >= 32 {
                    return None;
                }
                let raw = &text[start..index];
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let start = start + raw.len() - raw.trim_start().len();
                output.push(Span {
                    text: trimmed,
                    inner: start..start + trimmed.len(),
                    end: index + ch.len_utf8(),
                });
            }
            _ => {}
        }
    }
    open.is_none().then_some(output)
}

fn target_id(text: &str) -> Option<usize> {
    TERMS.iter().position(|(source, target)| {
        text == *source
            || text.chars().filter(|c| !c.is_whitespace()).eq(target.chars().filter(|c| !c.is_whitespace()))
            // A known model sense confusion, qualified by the source job span.
            || (*source == "勇者" && text == "용기")
    })
}

fn short_noun(text: &str) -> bool {
    text.chars().count() <= 24
        && text.split_whitespace().count() <= 3
        && text.chars().all(|c| c.is_alphabetic() || c.is_whitespace() || c == '・')
        // Do not erase a particle that the model moved inside the brackets.
        && !["은", "는", "을", "를", "과", "와", "으로", "에서", "에게", "이다", "라고"]
            .iter()
            .any(|suffix| text.ends_with(suffix))
}

fn particle<'a>(tail: &'a str, replacement: &str) -> Option<(&'a str, &'static str)> {
    let last = replacement.chars().last()?;
    if !('가'..='힣').contains(&last) {
        return None;
    }
    let jong = (last as u32 - '가' as u32) % 28;
    for (old, consonant, vowel) in [
        ("이라는", "이라는", "라는"),
        ("라는", "이라는", "라는"),
        ("이라고", "이라고", "라고"),
        ("라고", "이라고", "라고"),
        ("이란", "이란", "란"),
        ("란", "이란", "란"),
        ("이야", "이야", "야"),
        ("야", "이야", "야"),
        ("으로", "으로", "로"),
        ("은", "은", "는"),
        ("는", "은", "는"),
        ("이", "이", "가"),
        ("가", "이", "가"),
        ("을", "을", "를"),
        ("를", "을", "를"),
        ("과", "과", "와"),
        ("와", "과", "와"),
        ("로", "으로", "로"),
    ] {
        if let Some(rest) = tail.strip_prefix(old) {
            if rest
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || ".,!?;:。！？、…()[]{}\"'“”‘’".contains(c))
            {
                let new = if jong == 0 || (old == "으로" || old == "로") && jong == 8 {
                    vowel
                } else {
                    consonant
                };
                return Some((&tail[..old.len()], new));
            }
        }
    }
    None
}

/// Restore only explicit known job spans. Callers must protect approved user
/// terminology before invoking this helper; it does not own correction memory.
pub(crate) fn restore<'a>(source: &str, translated: &'a str) -> Cow<'a, str> {
    if source.contains(['\n', '\r']) || translated.contains(['\n', '\r']) {
        return Cow::Borrowed(translated);
    }
    let Some(source_spans) = spans(source).filter(|s| !s.is_empty()) else {
        return Cow::Borrowed(translated);
    };
    let Some(target_spans) = spans(translated).filter(|s| s.len() == source_spans.len()) else {
        return Cow::Borrowed(translated);
    };
    let Some(mut source_ids) = source_spans
        .iter()
        .map(|span| TERMS.iter().position(|(source, _)| *source == span.text))
        .collect::<Option<Vec<_>>>()
    else {
        return Cow::Borrowed(translated);
    };
    let target_ids = target_spans
        .iter()
        .map(|span| target_id(span.text))
        .collect::<Vec<_>>();
    let ids = if source_ids.iter().all(|&id| id == source_ids[0]) {
        let id = source_ids[0];
        if target_spans.iter().zip(target_ids).any(|(span, actual)| {
            actual.is_some_and(|actual| actual != id) || actual.is_none() && !short_noun(span.text)
        }) {
            return Cow::Borrowed(translated);
        }
        vec![id; target_spans.len()]
    } else {
        let Some(ids) = target_ids.into_iter().collect::<Option<Vec<_>>>() else {
            return Cow::Borrowed(translated);
        };
        let mut counts = ids.clone();
        source_ids.sort_unstable();
        counts.sort_unstable();
        if source_ids != counts {
            return Cow::Borrowed(translated);
        }
        ids
    };
    let mut edits = Vec::new();
    for (span, id) in target_spans.iter().zip(ids) {
        let canonical = TERMS[id].1;
        if span.text == canonical {
            continue;
        }
        let tail = &translated[span.end..];
        let adjustment = particle(tail, canonical);
        // Do not change a noun when its attached, unhandled inflection would
        // need a different consonant/vowel form. Never strip or invent it.
        let old_jong = span
            .text
            .chars()
            .last()
            .filter(|c| ('가'..='힣').contains(c))
            .map(|c| !(c as u32 - '가' as u32).is_multiple_of(28));
        let new_jong = canonical
            .chars()
            .last()
            .map(|c| !(c as u32 - '가' as u32).is_multiple_of(28));
        if old_jong != new_jong
            && adjustment.is_none()
            && ["이", "은", "을", "과", "으로"]
                .iter()
                .any(|p| tail.starts_with(p))
        {
            continue;
        }
        edits.push((span.inner.clone(), canonical));
        if let Some((old, new)) = adjustment {
            if old != new {
                edits.push((span.end..span.end + old.len(), new));
            }
        }
    }
    if edits.is_empty() {
        return Cow::Borrowed(translated);
    }
    let mut output = String::with_capacity(translated.len());
    let mut end = 0;
    for (range, replacement) in edits {
        output.push_str(&translated[end..range.start]);
        output.push_str(replacement);
        end = range.end;
    }
    output.push_str(&translated[end..]);
    Cow::Owned(output)
}

/// Expose the meaning of two established game abbreviations to inference while
/// retaining the original source for output formatting and user corrections.
pub(crate) fn expand(source: &str) -> Cow<'_, str> {
    if ["という言葉", "という表現", "略語", "略称", "の略"]
        .iter()
        .any(|s| source.contains(s))
    {
        return Cow::Borrowed(source);
    }
    let mut edits = Vec::new();
    for (old, new) in [
        ("ステ振り", "ステータスの割り振り"),
        ("レベ上げ", "レベル上げ"),
    ] {
        for (start, _) in source.match_indices(old) {
            let end = start + old.len();
            let before = source[..start].chars().next_back();
            let after = source[end..].chars().next();
            if before.is_some_and(|c| {
                ('ァ'..='ヿ').contains(&c) || c.is_ascii_alphanumeric() || c == '_'
            }) || after.is_some_and(|c| {
                ('ァ'..='ヿ').contains(&c)
                    || ('\u{3400}'..='\u{9fff}').contains(&c)
                    || c.is_ascii_alphabetic()
                    || c == '_'
            }) {
                continue;
            }
            edits.push((start..end, new));
        }
    }
    if edits.is_empty() {
        return Cow::Borrowed(source);
    }
    edits.sort_unstable_by_key(|(range, _)| range.start);
    let mut output = String::with_capacity(source.len());
    let mut end = 0;
    for (range, replacement) in edits {
        output.push_str(&source[end..range.start]);
        output.push_str(replacement);
        end = range.end;
    }
    output.push_str(&source[end..]);
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changes_only_qualified_bracket_payloads_and_preserves_courage_elsewhere() {
        assert_eq!(
            restore("【勇者】には勇気が必要だ。", "[용기]는 용기가 필요하다."),
            "[용사]는 용기가 필요하다."
        );
        assert_eq!(
            restore("彼には勇気がある。", "그는 용기가 있다."),
            "그는 용기가 있다."
        );
        assert_eq!(
            restore("【大拳闘士】になった。", "[대투자]가 되었다."),
            "[대권투사]가 되었다."
        );
        assert_eq!(
            restore("【筋肉戦士】になった。", "[육기 전사]가 되었다."),
            "[근육 전사]가 되었다."
        );
    }
    #[test]
    fn repairs_only_standalone_particles_after_a_changed_span() {
        assert_eq!(
            restore("【剣士】は静かだ。", "[검객]은 조용하다."),
            "[검사]는 조용하다."
        );
        assert_eq!(
            restore("【剣士】が来た。", "[검객]이 왔다."),
            "[검사]가 왔다."
        );
        assert_eq!(
            restore("【剣士】になった。", "[검객]이라는 직업이다."),
            "[검사]라는 직업이다."
        );
        assert_eq!(restore("【剣士】になった。", "[검객]은요."), "[검객]은요.");
        assert_eq!(restore("【勇者】は", "[용기는]"), "[용기는]");
    }
    #[test]
    fn mixed_lists_use_identified_target_names_not_positional_guessing() {
        assert_eq!(
            restore("【勇者】と【剣士】が来た。", "[검사]와 [용기]가 왔다."),
            "[검사]와 [용사]가 왔다."
        );
        for target in [
            "[알 수 없는 이름]과 [용기]가 왔다.",
            "[용기]와 [용기]가 왔다.",
            "[용기]가 왔다.",
        ] {
            assert_eq!(restore("【勇者】と【剣士】が来た。", target), target);
        }
    }
    #[test]
    fn lost_nested_unknown_or_numerical_spans_are_not_guessed() {
        for (source, target) in [
            ("【勇者】", "용기"),
            ("【勇者】", "[용기"),
            ("【勇者】", "[[용기]]"),
            ("【未知の職】", "[용기]"),
            ("【勇者】", "[검사]"),
            ("【勇者】", "[용기2]"),
            ("【勇者】\n【勇者】", "[용기]\n[용기]"),
            ("〈ダン活〉", "‘덴마크’"),
        ] {
            assert_eq!(restore(source, target), target);
        }
    }
    #[test]
    fn expands_abbreviations_without_altering_mentions_names_or_compounds() {
        assert_eq!(
            expand("ステ振りを相談して、レベ上げを始めた。"),
            "ステータスの割り振りを相談して、レベル上げを始めた。"
        );
        for source in [
            "ステ振りという言葉を説明する。",
            "ステ振りは略語だ。",
            "アステ振り",
            "Aステ振り",
            "レベ上げゲーム",
            "ステ振り直し",
            "〈ダン活〉を始めた。",
        ] {
            assert_eq!(expand(source), source);
        }
    }
}
