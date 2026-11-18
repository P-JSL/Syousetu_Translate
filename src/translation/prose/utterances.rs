//! Source-qualified repairs for complete short speech acts. A bare affirmative,
//! a clarification question and a hesitation are different utterances: do not
//! repair a word found inside a longer sentence or infer a speaker's persona.
use super::Utterance;
use std::borrow::Cow;

const TERMINAL: &[char] = &['。', '.', '！', '!', '？', '?', '…'];
const QUOTES: &[char] = &['「', '」', '『', '』', '“', '”', '‘', '’', '"'];

struct Marked<'a> {
    prefix: &'a str,
    word: &'a str,
    marks: &'a str,
}
impl<'a> Marked<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let text = text.trim();
        if text.contains(['\n', '\r']) || text.contains(QUOTES) {
            return None;
        }
        let after_prefix = text.trim_start_matches(['…', '.']);
        let prefix = &text[..text.len() - after_prefix.len()];
        let word = after_prefix.trim_end_matches(TERMINAL).trim_end();
        (!word.is_empty()).then_some(Self {
            prefix,
            word,
            marks: &after_prefix[word.len()..],
        })
    }
    fn question(&self) -> bool {
        self.marks.contains(['?', '？'])
    }
    fn hesitation(&self) -> bool {
        !self.prefix.is_empty() || self.marks.contains('…') || self.marks.contains("...")
    }
    fn render(&self, word: &str, implicit_question: bool) -> String {
        let mut result = String::with_capacity(self.prefix.len() + word.len() + self.marks.len());
        result.push_str(self.prefix);
        result.push_str(word);
        if self.marks.is_empty() && implicit_question {
            result.push('?');
        } else {
            for ch in self.marks.chars() {
                if ch.is_whitespace() {
                    continue;
                }
                result.push(match ch {
                    '。' => '.',
                    '！' => '!',
                    '？' => '?',
                    other => other,
                });
            }
        }
        result
    }
}

fn affirmative(text: &str) -> bool {
    matches!(
        text,
        "네" | "예" | "응" | "그래" | "그래요" | "그렇습니다" | "맞아" | "맞아요"
    )
}
fn negative(text: &str) -> bool {
    matches!(
        text,
        "아니요" | "아니오" | "아니" | "아니야" | "아니에요" | "아닙니다"
    )
}
fn mistaken_air(text: &str) -> bool {
    matches!(
        text,
        "공기 괜찮아요"
            | "공기 괜찮아"
            | "공기가 괜찮아요"
            | "공기가 괜찮아"
            | "공기는 괜찮아요"
            | "공기는 괜찮아"
            | "공기가 괜찮습니까"
            | "공기는 괜찮습니까"
    )
}

/// Repair only a complete, reviewed source cue paired with a known wrong
/// rendering. Keep richer translations, uncertain pauses and quoted definitions
/// intact. `typography` remains responsible for the outer quotation frame.
pub(super) fn repair<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let (Some(source), Some(generated)) = (Marked::parse(utterance.inner), Marked::parse(target))
    else {
        return Cow::Borrowed(target);
    };

    let corrected = if !source.question() && !source.hesitation() {
        match source.word {
            "ええ" | "はい" if negative(generated.word) => Some("네"),
            "うん"
                if negative(generated.word)
                    || matches!(
                        generated.word,
                        "네" | "예" | "그래요" | "그렇습니다" | "맞아요"
                    ) =>
            {
                Some("응")
            }
            "いいえ" if affirmative(generated.word) => Some("아니요"),
            "ううん" if affirmative(generated.word) => Some("아니"),
            _ => None,
        }
    } else {
        None
    };
    if let Some(corrected) = corrected {
        return Cow::Owned(source.render(corrected, false));
    }

    // A question mark, or an explicitly startled えっ!, distinguishes this
    // from an affirmative answer. Keep valid clarifications such as 네?/응?.
    let startled =
        source.marks.contains(['!', '！']) && matches!(source.word, "え" | "えっ" | "ええっ");
    if (source.question() || startled) && negative(generated.word) {
        let corrected = match source.word {
            "え" | "えっ" | "ええっ" => Some("어"),
            "ええ" | "はい" => Some("네"),
            "うん" => Some("응"),
            "は" | "はぁ" => Some("뭐"),
            _ => None,
        };
        if let Some(corrected) = corrected {
            return Cow::Owned(source.render(corrected, true));
        }
    }

    // 正気 concerns sanity, not 空気 (air). Scope to the observed incorrect
    // phrase and exact interrogative forms; negations and longer clauses do
    // not match. Do not replace valid synonyms such as 정신이 나갔어?.
    if mistaken_air(generated.word) {
        let corrected = match source.word {
            "正気かよ" | "正気なのか" | "正気なの" => Some("제정신이야"),
            "正気か" => Some(if utterance.is_dialogue() {
                "제정신이야"
            } else {
                "제정신인가"
            }),
            "正気ですか" | "正気なんですか" => Some("제정신이에요"),
            "正気" if source.question() => Some("제정신이야"),
            _ => None,
        };
        if let Some(corrected) = corrected {
            return Cow::Owned(source.render(corrected, true));
        }
    }
    Cow::Borrowed(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repair<'a>(source: &str, target: &'a str) -> Cow<'a, str> {
        super::repair(Utterance::parse(source), target)
    }

    #[test]
    fn complete_answers_keep_polarity_and_explicit_speech_register() {
        for (source, target, expected) in [
            ("「ええ」", "아니요", "네"),
            ("「はい。」", "아니오!", "네."),
            ("「ええ 。」", "아니요 .", "네."),
            ("「うん！」", "아니야.", "응!"),
            ("「うん！」", "그렇습니다!", "응!"),
            ("「いいえ。」", "네.", "아니요."),
            ("「ううん」", "그래", "아니"),
            ("うん。", "아니야.", "응."),
        ] {
            assert_eq!(repair(source, target), expected, "{source}");
            assert!(matches!(repair(source, expected), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn surprised_or_clarifying_questions_are_not_negative_answers() {
        for (source, target, expected) in [
            ("「え？」", "아니요", "어?"),
            ("「えっ！？」", "아니요?", "어!?"),
            ("「えっ！」", "아니요!", "어!"),
            ("「ええ？」", "아니요.", "네?"),
            ("「はい？」", "아니요.", "네?"),
            ("「うん？」", "아니야.", "응?"),
            ("「はぁ？」", "아니요.", "뭐?"),
            ("（……え？）", "아니요.", "……어?"),
        ] {
            assert_eq!(repair(source, target), expected, "{source}");
            assert!(matches!(repair(source, expected), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn sanity_question_repairs_air_without_flattening_dialogue_and_inner_monologue() {
        for (source, target, expected) in [
            ("「正気かよ！？」", "공기 괜찮아요!?", "제정신이야!?"),
            ("「正気か？」", "공기는 괜찮아?", "제정신이야?"),
            ("（正気か？）", "공기가 괜찮아요?", "제정신인가?"),
            ("正気か", "공기 괜찮아요", "제정신인가?"),
            ("「正気ですか？」", "공기는 괜찮습니까?", "제정신이에요?"),
            ("「……正気かよ……」", "공기가 괜찮아.", "……제정신이야……"),
        ] {
            assert_eq!(repair(source, target), expected, "{source}");
            assert!(matches!(repair(source, expected), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn hesitation_negation_meta_quotes_and_longer_contexts_are_not_blanket_replaced() {
        for source in [
            "「ええ……」",
            "「……ええ。」",
            "「ええと……」",
            "「ええ、でも違う。」",
            "「ええ、いいえ、どっちだろう？」",
            "「いいえ？」",
            "「うーん……」",
            "「ええ」とは言わない。",
            "『ええ』という言葉。",
            "彼はええと答えた。",
            "「ええ」\n「いや」",
        ] {
            assert_eq!(repair(source, "아니요."), "아니요.", "{source}");
        }
        for source in [
            "「正気ではない。」",
            "「正気かどうか分からない。」",
            "「本気かよ！？」",
            "「正気かよ、帰れ！」",
            "「正気かよ」という言葉。",
        ] {
            assert_eq!(
                repair(source, "공기 괜찮아요?"),
                "공기 괜찮아요?",
                "{source}"
            );
        }
    }

    #[test]
    fn valid_synonyms_richer_outputs_and_other_korean_negatives_are_preserved() {
        for (source, target) in [
            ("「ええ」", "물론이죠."),
            ("「ええ」", "아니요, 그럴 수 없어요."),
            ("「ええ」", "‘아니요’라는 말."),
            ("「ええ」", "아니요.\n아니에요."),
            ("「いいえ」", "아니요."),
            ("「え？」", "네?"),
            ("「うん？」", "응?"),
            ("「正気かよ！？」", "정신 나갔어!?"),
            ("「正気ですか？」", "제정신입니까?"),
            ("「正気かよ！？」", "공기는 괜찮지만, 제정신이야!?"),
        ] {
            assert_eq!(repair(source, target), target, "{source}");
        }
    }
}
