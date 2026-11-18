//! Repair a recognized clause only when both languages identify the same action.
use super::Utterance;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intent {
    Permission,
    Optional,
    Obligation,
    Prohibition,
}

use Intent::{Obligation, Optional, Permission, Prohibition};

struct Action {
    // Japanese connective, contraction and stem used before ない.
    japanese: [&'static str; 3],
    // Korean plain stem, 아/어 connective and adnominal form.
    korean: [&'static str; 3],
}

const ACTIONS: &[Action] = &[
    Action {
        japanese: ["飲んで", "飲んじゃ", "飲ま"],
        korean: ["마시", "마셔", "마실"],
    },
    Action {
        japanese: ["食べて", "食べちゃ", "食べ"],
        korean: ["먹", "먹어", "먹을"],
    },
    Action {
        japanese: ["読んで", "読んじゃ", "読ま"],
        korean: ["읽", "읽어", "읽을"],
    },
    Action {
        japanese: ["入って", "入っちゃ", "入ら"],
        korean: ["들어가", "들어가", "들어갈"],
    },
    Action {
        japanese: ["開けて", "開けちゃ", "開け"],
        korean: ["열", "열어", "열"],
    },
    Action {
        japanese: ["見て", "見ちゃ", "見"],
        korean: ["보", "봐", "볼"],
    },
    Action {
        japanese: ["急いで", "急いじゃ", "急が"],
        korean: ["서두르", "서둘러", "서두를"],
    },
    Action {
        japanese: ["行って", "行っちゃ", "行か"],
        korean: ["가", "가", "갈"],
    },
    Action {
        japanese: ["座って", "座っちゃ", "座ら"],
        korean: ["앉", "앉아", "앉을"],
    },
];

struct Source {
    action: &'static Action,
    intent: Intent,
    polite: bool,
    question: bool,
    full_stop: bool,
    without_pressure: bool,
}

const TERMINAL: [char; 8] = ['。', '.', '!', '?', '！', '？', '…', '．'];

fn source(utterance: Utterance<'_>) -> Option<Source> {
    if !utterance.is_dialogue() {
        return None;
    }
    let text = utterance.inner;
    let word = text.trim_end_matches(TERMINAL).trim_end();
    let marks = &text[word.len()..];
    let (stem, polite, interrogative) = [
        ("ですか", true, true),
        ("ですよね", true, false),
        ("ですよ", true, false),
        ("ですね", true, false),
        ("です", true, false),
        ("だよね", false, false),
        ("だよ", false, false),
        ("だね", false, false),
        ("なの", false, false),
        ("だ", false, false),
        ("よね", false, false),
        ("よ", false, false),
        ("ね", false, false),
        ("か", false, true),
        ("", false, false),
    ]
    .into_iter()
    .find_map(|(ending, polite, question)| {
        word.strip_suffix(ending)
            .map(|stem| (stem, polite, question))
    })?;
    let forms: &[(Intent, usize, &[&str])] = &[
        (
            Optional,
            2,
            &[
                "なくてもいい",
                "なくていい",
                "なくてもよい",
                "なくても大丈夫",
            ],
        ),
        (
            Obligation,
            2,
            &[
                "なければいけない",
                "なければならない",
                "なければなりません",
                "なくてはいけない",
                "なくてはならない",
                "なきゃいけない",
                "なきゃならない",
                "なくちゃいけない",
                "なくちゃならない",
                "なきゃだめ",
                "なきゃダメ",
                "なくちゃだめ",
                "なくちゃダメ",
            ],
        ),
        (
            Permission,
            0,
            &["もいい", "もよい", "も大丈夫", "いい", "よい"],
        ),
        (
            Prohibition,
            0,
            &[
                "はいけません",
                "はいけない",
                "はならない",
                "はだめ",
                "はダメ",
                "は駄目",
            ],
        ),
        (
            Prohibition,
            1,
            &["いけません", "いけない", "だめ", "ダメ", "駄目"],
        ),
    ];
    for &(intent, form, endings) in forms {
        for &ending in endings {
            let Some(predicate) = stem.strip_suffix(ending) else {
                continue;
            };
            for action in ACTIONS {
                let Some(prefix) = predicate.strip_suffix(action.japanese[form]) else {
                    continue;
                };
                if prefix.contains([
                    '。', '！', '？', '!', '?', '「', '」', '『', '』', '(', ')', '（', '）', '；',
                    ';',
                ]) || ["という", "っていう", "わけ", "とは", "けれど", "けど"]
                    .iter()
                    .any(|part| prefix.contains(part))
                {
                    return None;
                }
                return Some(Source {
                    action,
                    intent,
                    polite: polite || stem.ends_with("ません"),
                    question: interrogative || marks.contains(['?', '？']),
                    full_stop: marks.contains(['。', '.']),
                    without_pressure: intent == Optional && prefix.ends_with("無理に"),
                });
            }
        }
    }
    None
}

struct Target<'a> {
    prefix: &'a str,
    intent: Option<Intent>,
    ending: &'static str,
}

fn word_boundary(prefix: &str) -> bool {
    prefix
        .chars()
        .next_back()
        .is_none_or(|ch| ch.is_whitespace() || matches!(ch, ',' | '…'))
}

fn observe<'a>(text: &'a str, action: &Action) -> Option<Target<'a>> {
    let patterns: &[(Option<Intent>, usize, &[&str])] = &[
        (
            Some(Optional),
            0,
            &[
                "지 않아도 돼",
                "지 않아도 돼요",
                "지 않아도 된다",
                "지 않아도 됩니다",
                "지 않아도 괜찮아",
                "지 않아도 괜찮아요",
                "지 않아도 되나요",
            ],
        ),
        (
            Some(Optional),
            2,
            &[
                " 필요가 없다",
                " 필요가 없어",
                " 필요가 없어요",
                " 필요 없어요",
            ],
        ),
        (
            Some(Prohibition),
            0,
            &[
                "지 말아야 해",
                "지 말아야 해요",
                "지 말아야 한다",
                "지 말아야 합니다",
                "지 마",
                "지 마세요",
                "지 말라",
                "지 말아라",
                "지 마십시오",
            ],
        ),
        (
            Some(Prohibition),
            1,
            &[
                "서는 안 돼",
                "서는 안 돼요",
                "서는 안 된다",
                "서는 안 됩니다",
                "도 안 돼",
                "도 안 돼요",
                "도 안 되나요",
            ],
        ),
        (
            Some(Permission),
            1,
            &[
                "도 돼",
                "도 돼요",
                "도 된다",
                "도 됩니다",
                "도 되나요",
                "도 될까요",
                "도 좋아",
                "도 좋아요",
                "도 좋다",
                "도 괜찮아",
                "도 괜찮아요",
            ],
        ),
        (
            Some(Obligation),
            1,
            &["야 해", "야 해요", "야 한다", "야 합니다", "야 하나요"],
        ),
        (None, 1, &["야 하지 않아", "야 할 거야"]),
        (
            None,
            2,
            &[
                " 수 있어",
                " 수 있어요",
                " 수 있나요",
                " 수 있습니다",
                " 수 있습니까",
                " 수 없어",
                " 수 없어요",
                " 수 없나요",
                " 수 없습니다",
                " 수 없습니까",
            ],
        ),
    ];
    for &(intent, form, endings) in patterns {
        for &ending in endings {
            let Some(prefix) = text
                .strip_suffix(ending)
                .and_then(|stem| stem.strip_suffix(action.korean[form]))
                .filter(|prefix| word_boundary(prefix))
            else {
                continue;
            };
            return Some(Target {
                prefix,
                intent,
                ending,
            });
        }
    }
    None
}

pub(super) fn repair<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let Some(source) = source(utterance) else {
        return Cow::Borrowed(target);
    };
    let body = target.trim_end_matches(TERMINAL).trim_end();
    let marks = &target[body.len()..];
    let Some(observed) = observe(body, source.action) else {
        return Cow::Borrowed(target);
    };
    // The isolated prefix "안" on a negative command reverses its polarity.
    let duplicate = (source.intent == Prohibition && observed.intent == Some(Prohibition))
        .then(|| {
            observed
                .prefix
                .strip_suffix("안 ")
                .filter(|prefix| word_boundary(prefix))
        })
        .flatten();
    let target_question = marks.contains(['?', '？'])
        || ["나요", "까요", "습니까"]
            .iter()
            .any(|suffix| observed.ending.ends_with(suffix));
    let harsh_command =
        source.polite && matches!(observed.ending, "지 말아라" | "지 말라" | "지 마");
    let missing_qualifier = source.without_pressure
        && !["무리", "억지", "굳이"]
            .iter()
            .any(|word| observed.prefix.contains(word));
    if observed.intent == Some(source.intent)
        && source.question == target_question
        && duplicate.is_none()
        && !harsh_command
        && !missing_qualifier
    {
        return Cow::Borrowed(target);
    }
    // Keep unanalysed negative or scope markers outside the matched predicate.
    let prefix = duplicate.unwrap_or(observed.prefix);
    if ["안 ", "못 "]
        .iter()
        .any(|negative| prefix.strip_suffix(negative).is_some_and(word_boundary))
    {
        return Cow::Borrowed(target);
    }
    let [stem, connective, _] = source.action.korean;
    let (verb, ending) = match (source.intent, source.polite, source.question) {
        (Permission, true, true) => (connective, "도 될까요"),
        (Permission, false, true) => (connective, "도 돼"),
        (Permission, true, false) => (connective, "도 돼요"),
        (Permission, false, false) => (connective, "도 돼"),
        (Optional, true, true) => (stem, "지 않아도 되나요"),
        (Optional, true, false) => (stem, "지 않아도 돼요"),
        (Optional, false, _) => (stem, "지 않아도 돼"),
        (Obligation, true, true) => (connective, "야 하나요"),
        (Obligation, true, false) => (connective, "야 해요"),
        (Obligation, false, _) => (connective, "야 해"),
        (Prohibition, true, true) => (connective, "도 안 되나요"),
        (Prohibition, false, true) => (connective, "도 안 돼"),
        (Prohibition, true, false) => (stem, "지 마세요"),
        (Prohibition, false, false) => (stem, "지 마"),
    };
    let ending_marks = if source.question {
        "?"
    } else if source.full_stop {
        "."
    } else {
        ""
    };
    let qualifier = if missing_qualifier {
        "무리해서 "
    } else {
        ""
    };
    Cow::Owned(format!("{prefix}{qualifier}{verb}{ending}{ending_marks}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_permission_option_obligation_and_prohibition_in_context() {
        for (source, target, expected) in [
            (
                "「今日はその本を読まなくてもいいよ。」",
                "오늘은 그 책을 읽어서는 안 돼.",
                "오늘은 그 책을 읽지 않아도 돼.",
            ),
            (
                "「もう中に入ってもいいですか？」",
                "제가 안으로 들어갈 수 있어요?",
                "제가 안으로 들어가도 될까요?",
            ),
            (
                "「この水を飲まなきゃだめです。」",
                "이 물을 마셔야 하지 않아.",
                "이 물을 마셔야 해요.",
            ),
            (
                "「いいえ、まだ中に入っちゃダメです。」",
                "아니오, 아직 안 들어가지 마세요.",
                "아니오, 아직 들어가지 마세요.",
            ),
            (
                "「この窓を開けてはいけません。」",
                "이 창문을 열지 말아라.",
                "이 창문을 열지 마세요.",
            ),
        ] {
            assert_eq!(repair(Utterance::parse(source), target), expected);
        }
    }

    #[test]
    fn leaves_a_different_action_ambiguous_scope_and_correct_translation_alone() {
        for (source, target) in [
            ("「今日は読まなくてもいいよ。」", "오늘은 마시지 마."),
            ("「入っちゃだめではない。」", "들어가지 않아도 돼."),
            ("「聞いては困るけど、見てもいいよ。」", "봐서는 안 돼."),
            ("「その水を飲んでいいですよ。」", "그 물을 마셔도 돼요."),
            ("「この窓を開けてはだめなの？」", "이 창문을 열어도 안 돼?"),
        ] {
            assert_eq!(repair(Utterance::parse(source), target), target);
        }
    }
}
