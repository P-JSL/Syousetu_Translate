use super::{alignment, Utterance};
use std::borrow::Cow;

fn opening(ch: char) -> bool {
    matches!(ch, '(' | '[' | '“' | '‘' | '「' | '『')
}

fn closing(ch: char) -> bool {
    matches!(
        ch,
        '.' | ',' | '!' | '?' | '。' | '、' | '！' | '？' | ')' | ']' | '”' | '’' | '」' | '』'
    )
}

fn squeeze(text: &str) -> Cow<'_, str> {
    let text = text.trim();
    let mut previous = None;
    if text.chars().all(|ch| {
        let regular = if ch.is_whitespace() {
            ch == ' ' && previous.is_some_and(|ch| ch != ' ' && !opening(ch))
        } else {
            previous != Some(' ') || !closing(ch)
        };
        previous = Some(ch);
        regular
    }) {
        return Cow::Borrowed(text);
    }
    let mut output = String::with_capacity(text.len());
    let mut pending = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending = !output.is_empty();
            continue;
        }
        if pending && !closing(ch) && !output.chars().next_back().is_some_and(opening) {
            output.push(' ');
        }
        pending = false;
        output.push(ch);
    }
    Cow::Owned(output)
}
fn unwrap_target(text: &str) -> &str {
    for (open, close) in [
        ('“', '”'),
        ('‘', '’'),
        ('「', '」'),
        ('『', '』'),
        ('"', '"'),
        ('(', ')'),
        ('（', '）'),
    ] {
        let prefix = text.strip_prefix(open);
        let content = prefix.unwrap_or(text);
        let mut depth = 0usize;
        let mut fallback = None;
        for (end, ch) in content.char_indices() {
            if open != close && ch == open {
                depth += 1;
            }
            if ch != close {
                continue;
            }
            let nested = depth > 0;
            depth = depth.saturating_sub(1);
            if content[end + ch.len_utf8()..].chars().all(|tail| {
                tail.is_whitespace()
                    || matches!(
                        tail,
                        '.' | '!' | '?' | '。' | '！' | '？' | '…' | '”' | '’' | '」' | '』' | '"'
                    )
            }) {
                if prefix.is_none() || !nested {
                    return content[..end].trim();
                }
                fallback = Some(content[..end].trim());
            }
        }
        if let Some(content) = fallback {
            return content;
        }
    }
    text
}
fn remove_unmatched_opening<'a>(utterance: Utterance<'_>, mut text: &'a str) -> &'a str {
    // A source with an explicit nested quotation owns that boundary. Do not
    // mistake it for a model-generated extra opening quote.
    if utterance
        .inner
        .starts_with(['「', '『', '“', '‘', '"', '(', '（'])
    {
        return text;
    }
    for (open, close) in [
        ('“', '”'),
        ('‘', '’'),
        ('「', '」'),
        ('『', '』'),
        ('(', ')'),
        ('（', '）'),
    ] {
        let excess = text
            .matches(open)
            .count()
            .saturating_sub(text.matches(close).count());
        for _ in 0..excess {
            let Some(content) = text.strip_prefix(open) else {
                break;
            };
            text = content.trim_start();
        }
    }
    text
}
fn marks(text: &str) -> Cow<'_, str> {
    if text.contains("...") || text.contains(['！', '？']) {
        Cow::Owned(
            text.replace("...", "…")
                .replace('！', "!")
                .replace('？', "?"),
        )
    } else {
        Cow::Borrowed(text)
    }
}

fn restore_marks<'a>(inner: &str, mut text: Cow<'a, str>) -> Cow<'a, str> {
    let prefix_start = inner.trim_start_matches(['…', '.']);
    let prefix = &inner[..inner.len() - prefix_start.len()];
    if prefix.contains('…') || prefix.len() >= 3 {
        let prefix = marks(prefix);
        let body = text.trim_start_matches(['…', '.']).trim_start();
        if text.strip_prefix(prefix.as_ref()) != Some(body) {
            text = Cow::Owned(format!("{prefix}{body}"));
        }
    }
    let body = inner.trim_end_matches(['!', '?', '！', '？', '…', '.']);
    let terminal = &inner[body.len()..];
    if terminal.contains(['!', '?', '！', '？', '…']) || terminal.len() >= 3 {
        let terminal = marks(terminal);
        let body = text
            .trim_end_matches(['.', '!', '?', '。', '！', '？', '…'])
            .trim_end();
        if text.strip_suffix(terminal.as_ref()) != Some(body) {
            text = Cow::Owned(format!("{body}{terminal}"));
        }
    }
    text
}

fn render_leaf<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let mut text = Cow::Borrowed(target);
    for edit in [
        super::meaning::completion,
        super::meaning::request,
        super::meaning::formal_apology,
        super::register::render,
        super::modality::repair,
    ] {
        if let Cow::Owned(edited) = edit(utterance, &text) {
            text = Cow::Owned(edited);
        }
    }
    if let Some(edited) = super::address::restore(utterance, &text) {
        text = Cow::Owned(edited);
    }
    if let Some(edited) = super::relations::restore(utterance, &text) {
        text = Cow::Owned(edited);
    }
    restore_marks(utterance.inner, text)
}

fn render_scope<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let source = utterance.inner;
    if alignment::contains_quote(source) || alignment::contains_quote(target) {
        return restore_marks(source, Cow::Borrowed(target));
    }
    if alignment::sentences(source).nth(1).is_none()
        && alignment::sentences(target).nth(1).is_none()
    {
        return render_leaf(utterance, target);
    }
    if !alignment::aligned_sentences(source, target) {
        if let Some([(first, left), (second, right)]) =
            alignment::merged_dialogue(utterance, target)
        {
            let left = render_leaf(first, left);
            let right = render_leaf(second, right);
            let stop = if left.ends_with(['?', '!', '.', '…', '。']) {
                ""
            } else {
                "."
            };
            return Cow::Owned(format!("{left}{stop} {right}"));
        }
        return restore_marks(source, super::register::render_uniform(utterance, target));
    }
    let mut result = String::with_capacity(target.len());
    for (source, target) in alignment::sentences(source).zip(alignment::sentences(target)) {
        let clause = Utterance {
            inner: source,
            ..utterance
        };
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&render_leaf(clause, target));
    }
    Cow::Owned(result)
}

pub fn finish(source: &str, generated: &str) -> String {
    let utterance = Utterance::parse(source);
    let squeezed = squeeze(generated);
    if utterance.quotes.is_some() {
        let body = remove_unmatched_opening(utterance, unwrap_target(&squeezed));
        return utterance.frame(&render_scope(utterance, body));
    }
    if let Some((clause, span)) = alignment::embedded(utterance.inner, &squeezed) {
        if let Cow::Owned(edited) = render_scope(clause, &squeezed[span.clone()]) {
            let close_width = squeezed[span.end..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            let direct_particle = squeezed[span.end + close_width..].starts_with("고 ")
                && squeezed[span.clone()].ends_with(['다', '라'])
                && !edited.ends_with(['다', '라']);
            let particle = span.start + edited.len() + close_width;
            let mut output = squeezed.into_owned();
            output.replace_range(span, &edited);
            if direct_particle {
                output.insert(particle, '라');
            }
            return output;
        }
        return squeezed.into_owned();
    }
    match render_scope(utterance, &squeezed) {
        Cow::Owned(edited) => edited,
        Cow::Borrowed(_) => squeezed.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_openings_are_removed_without_losing_balanced_nested_quotes() {
        assert_eq!(
            finish(
                "「ボブはこの町には住んでないよ。」",
                "““바브는 이 도시에 살지 않는다.”"
            ),
            "“바브는 이 도시에 살지 않아.”"
        );
        assert_eq!(
            finish(
                "「『待って』と彼は言った。」",
                "“‘기다려’라고 그가 말했다.”"
            ),
            "“‘기다려’라고 그가 말했다.”"
        );
        assert_eq!(
            finish("「『待って』という言葉。」", "““기다려”라는 말.”"),
            "““기다려”라는 말.”"
        );
    }
    #[test]
    fn an_explicit_royal_vocative_is_not_replaced_with_an_unrelated_phrase() {
        assert_eq!(
            finish(
                "「陛下、ご命令をお待ちしております。」",
                "우리 영광스럽게도, 당신의 명령을 기다리고 있습니다."
            ),
            "“폐하, 당신의 명령을 기다리고 있습니다.”"
        );
    }
    #[test]
    fn generated_spacing_is_cleaned_without_joining_words_or_changing_numbers() {
        assert_eq!(
            finish(
                "重さは1.5 kgだ。",
                "  New  York의 가격은\n 1.5 kg 입니다 .  "
            ),
            "New York의 가격은 1.5 kg이다."
        );
        assert_eq!(
            finish("説明。", "그녀는\n\n 문을 열었다."),
            "그녀는 문을 열었다."
        );
    }
    #[test]
    fn source_pauses_questions_and_exclamations_are_preserved_without_invented_words() {
        assert_eq!(finish("「……大丈夫？」", "“괜찮아.”"), "“……괜찮아?”");
        assert_eq!(finish("「待って！」", "기다려 ."), "“기다려!”");
        assert_eq!(
            finish("彼は「待って」と言った。", "그는 ‘기다려’라고 말했다."),
            "그는 ‘기다려’라고 말했다."
        );
        assert_eq!(finish("「マジかよ……」", "진짜야?"), "“진짜야……”");
        assert_eq!(
            finish("「読んどいてくれない？」", "“내일까지 읽어보세요.”?”"),
            "“내일까지 읽어 놔 줄래?”"
        );
        assert_eq!(
            finish("「もう食べちゃったんだよ……」", "그럼 이제 다 먹어봤어요..."),
            "“그럼 이제 다 먹어버렸어……”"
        );
    }
}
