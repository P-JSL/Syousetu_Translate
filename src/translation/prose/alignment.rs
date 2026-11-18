//! Conservative, ordered sentence and quotation boundaries for prose editing.
use super::Utterance;
use std::ops::Range;

const QUOTES: [char; 9] = ['「', '」', '『', '』', '“', '”', '‘', '’', '"'];

pub(super) fn contains_quote(text: &str) -> bool {
    text.contains(QUOTES)
}

/// Periods inside numbers, Latin abbreviations and ellipses are not boundaries.
pub(super) fn sentences(text: &str) -> impl Iterator<Item = &str> {
    let mut remaining = text.trim();
    std::iter::from_fn(move || {
        if remaining.is_empty() {
            return None;
        }
        let mut chars = remaining.char_indices().peekable();
        let mut previous = None;
        while let Some((offset, ch)) = chars.next() {
            let stop = matches!(ch, '。' | '！' | '？' | '!' | '?')
                || (ch == '.'
                    && previous
                        .is_some_and(|prev: char| prev != '.' && !prev.is_ascii_alphanumeric())
                    && chars.peek().is_none_or(|&(_, next)| next != '.'));
            if stop {
                let mut end = offset + ch.len_utf8();
                while let Some((index, mark)) = chars.next_if(|&(_, mark)| {
                    matches!(mark, '。' | '！' | '？' | '.' | '!' | '?' | '…')
                }) {
                    end = index + mark.len_utf8();
                }
                let (body, rest) = remaining.split_at(end);
                remaining = rest.trim_start();
                return Some(body);
            }
            previous = Some(ch);
        }
        Some(std::mem::take(&mut remaining))
    })
}

fn question(text: &str) -> bool {
    text.trim_end_matches(['.', '。', '…', '!', '！'])
        .ends_with(['?', '？'])
}

fn compatible_content(source: &str, target: &str) -> bool {
    fn numbers(text: &str) -> impl Iterator<Item = &str> {
        text.split(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '.' | ',')))
            .map(|part| part.trim_matches(['.', ',']))
            .filter(|part| !part.is_empty())
    }
    source.chars().any(char::is_alphanumeric)
        && target.chars().any(char::is_alphanumeric)
        && numbers(source).eq(numbers(target))
}

fn compatible(source: &str, target: &str) -> bool {
    question(source) == question(target) && compatible_content(source, target)
}

pub(super) fn aligned_sentences(source: &str, target: &str) -> bool {
    let count = sentences(source).count();
    (2..=8).contains(&count)
        && count == sentences(target).count()
        && sentences(source)
            .zip(sentences(target))
            .all(|(a, b)| compatible(a, b))
}

/// A model sometimes replaces the full stop between two speech registers
/// with a comma. Restore it when both sides contain a complete predicate.
pub(super) fn merged_dialogue<'s, 't>(
    utterance: Utterance<'s>,
    target: &'t str,
) -> Option<[(Utterance<'s>, &'t str); 2]> {
    if !utterance.is_dialogue() {
        return None;
    }
    let mut clauses = sentences(utterance.inner);
    let first = Utterance {
        inner: clauses.next()?,
        ..utterance
    };
    let second = Utterance {
        inner: clauses.next()?,
        ..utterance
    };
    if clauses.next().is_some() || sentences(target).nth(1).is_some() {
        return None;
    }
    let (left, right) = target.rsplit_once(',')?;
    let left = left.trim_end();
    let right = right.trim_start();
    if let Some((interjection, rest)) = left.split_once(',') {
        if !["아", "아니", "아니요", "응", "네", "음", "그래"].contains(&interjection.trim())
            || rest.contains(',')
        {
            return None;
        }
    }
    let omitted_question = question(first.inner)
        && first
            .inner
            .trim_end_matches(['?', '？'])
            .ends_with("大丈夫")
        && ["괜찮아", "괜찮아요", "괜찮습니다"]
            .iter()
            .any(|ending| left.ends_with(ending))
        && compatible_content(first.inner, left);
    let mixed = first.inner.ends_with(['。', '.'])
        && super::register::contrasting_dialogue(first, second)
        && compatible(first.inner, left);
    if !(omitted_question || mixed)
        || !compatible(second.inner, right)
        || !super::register::finite_dialogue(left)
        || !super::register::finite_dialogue(right)
    {
        return None;
    }
    Some([(first, left), (second, right)])
}

fn only_quote(text: &str, pairs: &[(char, char)]) -> Option<(Range<usize>, char)> {
    let start = text.find(QUOTES)?;
    let (open, close) = pairs
        .iter()
        .copied()
        .find(|&(open, _)| text[start..].starts_with(open))?;
    let first = start + open.len_utf8();
    let end = first + text[first..].find(QUOTES)?;
    if !text[end..].starts_with(close)
        || contains_quote(&text[end + close.len_utf8()..])
        || text[first..end].trim().is_empty()
    {
        return None;
    }
    Some((first..end, close))
}

/// Match one direct quotation embedded in narration. The target keeps its own
/// position and quotation marks, which can differ under Korean word order.
pub(super) fn embedded<'a>(source: &'a str, target: &str) -> Option<(Utterance<'a>, Range<usize>)> {
    let (source_range, close) = only_quote(source, &[('「', '」'), ('『', '』')])?;
    let suffix = source[source_range.end + close.len_utf8()..].trim_start();
    if !(suffix.starts_with('と') || suffix.starts_with("って"))
        || ["という言葉", "という表現", "という単語", "っていう言葉"]
            .iter()
            .any(|prefix| suffix.starts_with(prefix))
        || suffix == "と"
        || suffix == "って"
    {
        return None;
    }
    let (target_range, target_close) = only_quote(
        target,
        &[
            ('“', '”'),
            ('‘', '’'),
            ('"', '"'),
            ('「', '」'),
            ('『', '』'),
        ],
    )?;
    let target_has_narration = target[..target_range.start]
        .chars()
        .any(char::is_alphanumeric)
        || target[target_range.end + target_close.len_utf8()..]
            .chars()
            .any(char::is_alphanumeric);
    target_has_narration.then_some((
        Utterance {
            inner: source[source_range].trim(),
            quotes: Some(if close == '」' {
                ('“', '”')
            } else {
                ('‘', '’')
            }),
        },
        target_range,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_alignment_keeps_decimal_and_abbreviation_boundaries() {
        assert_eq!(
            sentences("Mr. 김은 3.14를 썼어. 다음은 5.0 kg이야!").collect::<Vec<_>>(),
            ["Mr. 김은 3.14를 썼어.", "다음은 5.0 kg이야!"]
        );
        assert!(!aligned_sentences("本当？ そうだよ。", "정말이야. 그래."));
        assert!(aligned_sentences("本当？ そうだよ。", "정말? 그래."));
    }

    #[test]
    fn embedded_alignment_requires_one_balanced_direct_quote() {
        let (utterance, range) =
            embedded("「待って」と彼が言った。", "그가 “기다려”라고 말했다.").unwrap();
        assert_eq!(utterance.inner, "待って");
        assert_eq!(&"그가 “기다려”라고 말했다."[range], "기다려");
        assert!(embedded("「待って」という言葉。", "“기다려”라는 말.").is_none());
        assert!(embedded("「一」と「二」と言った。", "“하나”와 “둘”이라고 말했다.").is_none());
        assert!(embedded("「待って」と彼が言った。", "“기다려.”").is_none());
    }
}
