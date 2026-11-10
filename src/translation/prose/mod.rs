//! Source-aware typography. It preserves words, units and speech register.
pub mod japanese;
mod meaning;
mod register;
mod typography;
pub use typography::finish;

#[derive(Clone, Copy, Debug)]
pub struct Utterance<'a> {
    pub inner: &'a str,
    pub quotes: Option<(char, char)>,
}
impl<'a> Utterance<'a> {
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
