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
        let content = text.strip_prefix(open).unwrap_or(text);
        for (end, ch) in content.char_indices() {
            if ch == close
                && content[end + ch.len_utf8()..].chars().all(|tail| {
                    tail.is_whitespace()
                        || matches!(
                            tail,
                            '.' | '!'
                                | '?'
                                | '。'
                                | '！'
                                | '？'
                                | '…'
                                | '”'
                                | '’'
                                | '」'
                                | '』'
                                | '"'
                        )
                })
            {
                return content[..end].trim();
            }
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
        unwrap_target(&squeezed).to_owned()
    } else {
        squeezed
    };
    let text = super::meaning::completion(source, &text);
    let text = super::meaning::request(source, &text);
    let mut text = super::register::render(source, &text);
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
