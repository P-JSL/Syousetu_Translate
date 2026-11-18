//! Explicit colloquial prohibitions with their source scope and speech level.
use super::super::Utterance;

struct Action {
    contracted: &'static str,
    connective: &'static str,
    command: Option<&'static str>,
}
const ACTIONS: &[Action] = &[
    Action {
        contracted: "飲んじゃ",
        connective: "飲んで",
        command: None,
    },
    Action {
        contracted: "食べちゃ",
        connective: "食べて",
        command: None,
    },
    Action {
        contracted: "開けちゃ",
        connective: "開けて",
        command: None,
    },
    Action {
        contracted: "触っちゃ",
        connective: "触って",
        command: None,
    },
    Action {
        contracted: "見ちゃ",
        connective: "見て",
        command: None,
    },
    // A separate real-model trial found the connective's double negative.
    Action {
        contracted: "入っちゃ",
        connective: "入って",
        command: Some("入らないでください"),
    },
];
const ENDINGS: &[(&str, &str)] = &[
    ("ですよ", "はいけませんよ"),
    ("です", "はいけません"),
    ("なんだ", "はいけないんだ"),
    ("だって", "はいけないって"),
    ("だよ", "はいけないよ"),
    ("だね", "はいけないね"),
    ("だぞ", "はいけないぞ"),
    ("なの", "はいけないの"),
    ("だ", "はいけない"),
    ("", "はいけない"),
];
struct Prohibition<'a> {
    open: char,
    close: char,
    prefix: &'a str,
    action: &'static Action,
    ending: &'static str,
    marks: &'a str,
}
fn parse(utterance: Utterance<'_>) -> Option<Prohibition<'_>> {
    let (open, close) = match utterance.quotes? {
        ('“', '”') => ('「', '」'),
        ('‘', '’') => ('『', '』'),
        _ => return None,
    };
    let inner = utterance.inner;
    let core = inner.trim_end_matches(['。', '！', '？', '!', '?', '…', '.']);
    let marks = &inner[core.len()..];
    for &(tail, ending) in ENDINGS {
        let Some(before_tail) = core.strip_suffix(tail) else {
            continue;
        };
        for negative in ["だめ", "ダメ", "駄目"] {
            let Some(before_negative) = before_tail.strip_suffix(negative) else {
                continue;
            };
            for action in ACTIONS {
                let Some(prefix) = before_negative.strip_suffix(action.contracted) else {
                    continue;
                };
                if prefix.contains([
                    '。', '！', '？', '!', '?', ';', '；', '\n', '「', '」', '『', '』', '“', '”',
                    '"',
                ]) || ["という表現", "という言葉", "っていう言葉"]
                    .iter()
                    .any(|part| prefix.contains(part))
                    || (action.command.is_some()
                        && (marks.contains(['?', '？'])
                            || matches!(tail, "なの" | "ですか" | "だって")))
                {
                    return None;
                }
                return Some(Prohibition {
                    open,
                    close,
                    prefix,
                    action,
                    ending,
                    marks,
                });
            }
        }
    }
    None
}
pub(super) fn matches(source: Utterance<'_>) -> bool {
    parse(source).is_some()
}
pub(super) fn expand(source: &str) -> Option<String> {
    let parsed = parse(Utterance::parse(source))?;
    let action = match parsed.action.command {
        Some(command) => command.to_owned(),
        None => format!("{}{}", parsed.action.connective, parsed.ending),
    };
    Some(format!(
        "{}{}{action}{}{}",
        parsed.open, parsed.prefix, parsed.marks, parsed.close
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_preserve_objects_questions_and_polite_forms() {
        for (source, expected) in [
            (
                "「その水は飲んじゃだめだよ。」",
                "「その水は飲んではいけないよ。」",
            ),
            ("「もう食べちゃだめなの？」", "「もう食べてはいけないの？」"),
            (
                "「絶対にドアを開けちゃダメ！」",
                "「絶対にドアを開けてはいけない！」",
            ),
            (
                "『それには触っちゃダメだって！』",
                "『それには触ってはいけないって！』",
            ),
            (
                "「今は画面を見ちゃ駄目です。」",
                "「今は画面を見てはいけません。」",
            ),
            (
                "「まだ中に入っちゃだめだよ。」",
                "「まだ中に入らないでください。」",
            ),
        ] {
            assert_eq!(expand(source).as_deref(), Some(expected));
        }
    }
    #[test]
    fn permission_negated_prohibition_quotes_and_untested_scope_are_untouched() {
        for source in [
            "「水を飲んでもいいよ。」",
            "「食べなきゃダメだよ。」",
            "「飲まなくちゃダメなんだ。」",
            "「飲まなくてもいいよ。」",
            "「飲んじゃだめではないよ。」",
            "「飲んじゃダメとは言ってないよ。」",
            "「『飲んじゃダメ』という言葉。」",
            "「その水を飲んじゃだめと言われた。」",
            "「飲んじゃだめだった。」",
            "「入っちゃダメなの？」",
            "「ここで食べちゃ駄目ですか？」",
            "「入っちゃダメだって。」",
            "「ドアを開けちゃダメ。中に入っちゃだめだよ！」",
            "飲んじゃダメだよ。",
            "「水を飲んじゃったよ。」",
            "「一」と「飲んじゃだめ」",
        ] {
            assert!(!matches(Utterance::parse(source)), "{source}");
            assert!(expand(source).is_none());
        }
    }
}
