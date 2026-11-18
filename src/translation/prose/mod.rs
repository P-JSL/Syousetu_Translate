//! Source-aware typography. It preserves words, units and speech register.
mod address;
mod alignment;
#[cfg(feature = "offline-model")]
pub(crate) mod game_terms;
pub mod japanese;
mod meaning;
mod modality;
#[cfg(feature = "offline-model")]
mod numbers;
mod register;
mod relations;
mod typography;
mod utterances;
use std::borrow::Cow;
pub use typography::finish;

#[cfg(feature = "offline-model")]
pub(crate) fn restore_facts(source: &str, mut translated: String) -> String {
    if source
        .chars()
        .any(|ch| ch.is_ascii_digit() || ('０'..='９').contains(&ch))
    {
        if let Cow::Owned(repaired) = numbers::repair(source, &translated) {
            translated = repaired;
        }
    }
    if source.contains('【') {
        if let Cow::Owned(repaired) = game_terms::restore(source, &translated) {
            translated = repaired;
        }
    }
    translated
}

/// Process matching source/output lines without guessing a new paragraph map.
pub fn finish_lines(source: &str, generated: &str) -> String {
    fn line<'a>(source: &str, generated: &'a str) -> Cow<'a, str> {
        if source
            .chars()
            .any(|ch| matches!(ch, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}'))
            && generated.chars().any(|ch| ('가'..='힣').contains(&ch))
        {
            Cow::Owned(finish(source, generated))
        } else {
            Cow::Borrowed(generated)
        }
    }
    if !source.contains('\n') {
        return line(source, generated).into_owned();
    }
    let pairs = || source.split('\n').zip(generated.split('\n'));
    if source.split('\n').count() != generated.split('\n').count()
        || pairs().any(|(source, target)| source.trim().is_empty() != target.trim().is_empty())
    {
        return generated.to_owned();
    }
    let mut output = String::with_capacity(generated.len());
    for (index, (source, target)) in pairs().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        let content = target.strip_suffix('\r').unwrap_or(target);
        output.push_str(&line(source, content));
        if content.len() != target.len() {
            output.push('\r');
        }
    }
    output
}

#[derive(Clone, Copy, Debug)]
pub struct Utterance<'a> {
    pub inner: &'a str,
    pub quotes: Option<(char, char)>,
}
impl<'a> Utterance<'a> {
    pub(super) fn is_dialogue(self) -> bool {
        matches!(self.quotes, Some(('“', '”') | ('‘', '’')))
    }

    pub fn parse(source: &'a str) -> Self {
        let source = source.trim();
        for (open, close, target) in [
            ('「', '」', ('“', '”')),
            ('『', '』', ('‘', '’')),
            ('（', '）', ('(', ')')),
        ] {
            if source.starts_with(open) && source.ends_with(close) {
                let mut depth = 0usize;
                let mut outer = true;
                for (offset, ch) in source.char_indices() {
                    if ch == open {
                        depth += 1;
                    }
                    if ch == close {
                        depth = depth.saturating_sub(1);
                        if depth == 0 && offset + ch.len_utf8() != source.len() {
                            outer = false;
                            break;
                        }
                    }
                }
                if outer && depth == 0 {
                    return Self {
                        inner: source[open.len_utf8()..source.len() - close.len_utf8()].trim(),
                        quotes: Some(target),
                    };
                }
            }
        }
        Self {
            inner: source,
            quotes: None,
        }
    }
    pub fn frame(&self, text: &str) -> String {
        match self.quotes {
            Some((open, close)) if text.starts_with(open) && text.ends_with(close) => {
                text.to_owned()
            }
            Some((open, close)) => format!("{open}{text}{close}"),
            None => text.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_narration_is_not_misclassified_as_one_quotation() {
        assert_eq!(Utterance::parse("「一」と「二」").quotes, None);
        assert_eq!(Utterance::parse("　「待って」").inner, "待って");
        assert_eq!(Utterance::parse("「外「内」外」").inner, "外「内」外");
    }
}
