use super::Utterance;

fn squeeze(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending = !output.is_empty();
            continue;
        }
        if pending
            && !matches!(
                ch,
                '.' | ','
                    | '!'
                    | '?'
                    | '。'
                    | '、'
                    | '！'
                    | '？'
                    | ')'
                    | ']'
                    | '”'
                    | '’'
                    | '」'
                    | '』'
            )
            && !output.ends_with(['(', '[', '“', '‘', '「', '『'])
        {
            output.push(' ');
        }
        pending = false;
        output.push(ch);
    }
    output
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
    if utterance.inner.starts_with(['「', '『', '“', '‘', '"']) {
        return text;
    }
    for (open, close) in [('“', '”'), ('‘', '’'), ('「', '」'), ('『', '』')] {
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
fn marks(text: &str) -> String {
    text.replace("...", "…")
        .replace('！', "!")
        .replace('？', "?")
}
pub fn finish(source: &str, generated: &str) -> String {
    let utterance = Utterance::parse(source);
    let squeezed = squeeze(generated);
    let text = if utterance.quotes.is_some() {
        remove_unmatched_opening(utterance, unwrap_target(&squeezed)).to_owned()
    } else {
        squeezed
    };
    let text = super::meaning::completion(source, &text);
    let text = super::meaning::request(source, &text);
    let text = super::meaning::formal_apology(source, &text);
    let mut text = super::register::render(source, &text);
    text = super::address::restore(utterance, &text).unwrap_or(text);
    text = super::relations::restore(utterance, &text).unwrap_or(text);
    let inner = utterance.inner;
    let prefix = inner
        .chars()
        .take_while(|ch| matches!(ch, '…' | '.'))
        .collect::<String>();
    if prefix.contains('…') || prefix.len() >= 3 {
        let rest = text.trim_start_matches(['…', '.']).trim_start();
        text = format!("{}{rest}", marks(&prefix));
    }
    let terminal = inner
        .chars()
        .rev()
        .take_while(|ch| matches!(ch, '!' | '?' | '！' | '？' | '…' | '.'))
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if terminal
        .chars()
        .any(|ch| matches!(ch, '!' | '?' | '！' | '？' | '…'))
        || terminal.len() >= 3
    {
        let rest = text
            .trim_end_matches(['.', '!', '?', '。', '！', '？', '…'])
            .trim_end();
        text = format!("{rest}{}", marks(&terminal));
    }
    utterance.frame(&text)
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
