//! Keep independently punctuated sentences in their original paragraph.
use crate::translation::prose::Utterance;

pub(super) struct Segments<'a> {
    pub(super) sentences: Vec<&'a str>,
    pub(super) framing: Option<Framing>,
}

#[derive(Clone, Copy)]
pub(super) struct Framing {
    pub(super) source: (char, char),
    pub(super) target: (char, char),
}

fn close_for(ch: char) -> Option<char> {
    match ch {
        '「' => Some('」'),
        '『' => Some('』'),
        '〈' => Some('〉'),
        '《' => Some('》'),
        '【' => Some('】'),
        '（' => Some('）'),
        '(' => Some(')'),
        '[' => Some(']'),
        '“' => Some('”'),
        '‘' => Some('’'),
        _ => None,
    }
}

fn terminal(ch: char) -> bool {
    matches!(ch, '。' | '！' | '？' | '!' | '?')
}

pub(super) fn segments(line: &str) -> Option<Segments<'_>> {
    let utterance = Utterance::parse(line);
    let mut sentences = Vec::new();
    let mut stack = Vec::new();
    let mut start = 0;
    let mut chars = utterance.inner.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if ch == '"' {
            if stack.last() == Some(&ch) {
                stack.pop();
            } else {
                stack.push(ch);
            }
        } else if let Some(close) = close_for(ch) {
            stack.push(close);
        } else if matches!(
            ch,
            '」' | '』' | '〉' | '》' | '】' | '）' | ')' | ']' | '”' | '’'
        ) && stack.pop() != Some(ch)
        {
            return None;
        }
        if stack.is_empty() && terminal(ch) {
            let mut end = offset + ch.len_utf8();
            while let Some((offset, mark)) =
                chars.next_if(|&(_, mark)| terminal(mark) || mark == '…')
            {
                end = offset + mark.len_utf8();
            }
            sentences.push(utterance.inner[start..end].trim());
            start = end;
        }
    }
    if !stack.is_empty() {
        return None;
    }
    let remainder = utterance.inner[start..].trim();
    if !remainder.is_empty() {
        sentences.push(remainder);
    }
    if sentences.len() < 2
        || sentences
            .iter()
            .any(|sentence| !crate::offline::text::japanese(sentence))
    {
        return None;
    }
    let framing = utterance.quotes.map(|target| {
        let line = line.trim();
        Framing {
            source: (
                line.chars().next().unwrap(),
                line.chars().next_back().unwrap(),
            ),
            target,
        }
    });
    Some(Segments { sentences, framing })
}

pub(super) fn may_have_missing_sentence(segments: &Segments<'_>, translation: &str) -> bool {
    let mut count = 0;
    let mut content = false;
    let mut quoted = false;
    let translation = translation.trim();
    let translation = segments
        .framing
        .and_then(|framing| {
            translation
                .strip_prefix(framing.target.0)?
                .strip_suffix(framing.target.1)
        })
        .unwrap_or(translation);
    for ch in translation.chars() {
        if matches!(ch, '“' | '‘' | '「' | '『') {
            quoted = true;
        } else if matches!(ch, '”' | '’' | '」' | '』') {
            quoted = false;
        }
        if (terminal(ch) || matches!(ch, '.' | '~' | '～' | '〜')) && !quoted {
            count += usize::from(content);
            content = false;
        } else {
            content |= ch.is_alphabetic();
        }
    }
    count += usize::from(content);
    let compact = |text: &str| text.chars().filter(|ch| ch.is_alphanumeric()).count();
    let original: usize = segments.sentences.iter().map(|text| compact(text)).sum();
    // Several Japanese sentences may legitimately become one Korean sentence.
    // Retry only when both the boundaries and the amount of text were reduced.
    count < segments.sentences.len() && compact(translation) * 5 < original * 4
}

pub(super) fn usable(source: &str, translation: &str) -> bool {
    if translation.trim().is_empty() {
        return false;
    }
    if translation.chars().any(|ch| ('가'..='힣').contains(&ch)) {
        return true;
    }
    // Support legitimate Latin product names and game codes. An isolated
    // Korean jamo sequence or a verbatim Japanese sentence needs review.
    !translation
        .chars()
        .any(|ch| matches!(ch, 'ᄀ'..='ᇿ' | 'ㄱ'..='ㆎ' | 'ぁ'..='ゖ' | 'ァ'..='ヺ' | 'ｦ'..='ﾟ'))
        && (translation.chars().any(|ch| ch.is_ascii_alphanumeric())
            || !source.chars().any(char::is_alphabetic))
}

pub(super) fn preserve_for_review(source: &str) -> String {
    format!("{} {}", crate::offline::text::REVIEW_MARKER, source.trim())
}

pub(super) fn recover_known_phrase(source: Utterance<'_>) -> Option<String> {
    let (prefix, body) = source.inner.split_once(['、', '，'])?;
    let laugh = match prefix.trim() {
        "ふふ" => "후후",
        "ふふふ" => "후후후",
        _ => return None,
    };
    let body = body.trim().strip_prefix("そんなに喜ばれると")?;
    let action = match body.strip_suffix("甲斐があります。")? {
        "作った" => "만든",
        "待った" => "기다린",
        "来た" => "찾아온",
        _ => return None,
    };
    Some(source.frame(&format!(
        "{laugh}, 그렇게 좋아해 주시니 {action} 보람이 있네요."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segmentation_respects_nested_quotes_pauses_and_decimal_numbers() {
        let parts = segments(" 「本当！？　『待って。』と言ったよ。 レベル1.5だ。」 ").unwrap();
        assert_eq!(
            parts.sentences,
            ["本当！？", "『待って。』と言ったよ。", "レベル1.5だ。"]
        );
        assert_eq!(parts.framing.unwrap().target, ('“', '”'));
        assert!(segments("彼は「本当？　そうだよ。」と言った。").is_none());
        assert!(segments("彼は「行こう。 今日は晴れだ。").is_none());
        assert!(segments("〈記録？〉です。 ……").is_none());
    }

    #[test]
    fn unusable_translations_can_keep_the_source_for_review() {
        assert!(!usable("これは本です。", ""));
        assert!(!usable("これは本です。", "ᄋᄃ ᅳ"));
        assert!(!usable("これは本です。", "これは本です。"));
        assert!(usable("ステータス", "HP 5"));
        assert!(usable("こんにちは", "안녕하세요."));
    }

    #[test]
    fn retry_requires_a_shorter_translation_and_fewer_sentence_boundaries() {
        let source = segments("ここまでは私の仕事です。残りはどうしますか？").unwrap();
        assert!(may_have_missing_sentence(&source, "여기까진 제 일입니다."));
        assert!(!may_have_missing_sentence(
            &source,
            "제 일은 여기까지인데, 나머지는 어떻게 하실 생각인가요?"
        ));
        assert!(!may_have_missing_sentence(
            &source,
            "제 일이에요. 어쩔까요?"
        ));
    }

    #[test]
    fn reviewed_worth_doing_phrase_keeps_the_laugh_and_action() {
        assert_eq!(
            recover_known_phrase(Utterance::parse(
                "「ふふ、そんなに喜ばれると待った甲斐があります。」"
            ))
            .as_deref(),
            Some("“후후, 그렇게 좋아해 주시니 기다린 보람이 있네요.”")
        );
        assert!(recover_known_phrase(Utterance::parse(
            "ふふ、そんなに喜ばれると作った甲斐はありません。"
        ))
        .is_none());
    }
}
