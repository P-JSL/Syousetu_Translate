//! Small, source-qualified aspect corrections. Never change an arbitrary
//! Korean "아/어 봤다" (which may genuinely describe an experience).
use std::borrow::Cow;
pub(super) fn request<'a>(source: &str, target: &'a str) -> Cow<'a, str> {
    if !super::japanese::friendly_request(source)
        || source.contains("ないで")
        || source.contains("なくて")
    {
        return Cow::Borrowed(target);
    }
    for (contracted, expanded, previous, corrected) in [
        ("読んどいて", "読んでおいて", "읽어보세요", "읽어 놔 줄래"),
        ("読んどいて", "読んでおいて", "읽어 보세요", "읽어 놔 줄래"),
        ("やっといて", "やっておいて", "해보세요", "해 놔 줄래"),
        ("やっといて", "やっておいて", "해 보세요", "해 놔 줄래"),
    ] {
        if (source.contains(contracted) || source.contains(expanded)) && target.contains(previous) {
            return Cow::Owned(target.replace(previous, corrected));
        }
    }
    if let Some(stem) = target
        .trim_end_matches(['.', '?', '!', '。', '！', '？', '…'])
        .strip_suffix("주세요")
    {
        return Cow::Owned(format!("{} 줄래", stem.trim_end()));
    }
    Cow::Borrowed(target)
}

pub(super) fn completion<'a>(source: &str, target: &'a str) -> Cow<'a, str> {
    let utterance = super::Utterance::parse(source);
    if utterance.quotes.is_none()
        || source.contains("てみ")
        || source.contains("試し")
        || utterance
            .inner
            .matches(['。', '!', '！', '?', '？'])
            .count()
            > 1
    {
        return Cow::Borrowed(target);
    }
    for (contraction, expanded, wrong, corrected) in [
        ("食べちゃった", "食べてしまった", "먹어봤", "먹어버렸"),
        ("飲んじゃった", "飲んでしまった", "마셔봤", "마셔버렸"),
        ("買っちゃった", "買ってしまった", "사봤", "사버렸"),
    ] {
        if (source.contains(contraction) || source.contains(expanded)) && target.contains(wrong) {
            return Cow::Owned(target.replace(wrong, corrected));
        }
    }
    Cow::Borrowed(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_is_not_mistaken_for_an_experience_or_applied_without_source_evidence() {
        assert_eq!(
            completion("「もう食べちゃったんだ」", "이미 먹어봤어요."),
            "이미 먹어버렸어요."
        );
        for source in [
            "「食べてみたよ」",
            "「試しに食べちゃったよ」",
            "旅行先で食べちゃった。",
        ] {
            assert_eq!(completion(source, "한번 먹어봤어."), "한번 먹어봤어.");
        }
    }
    #[test]
    fn friendly_request_is_not_a_third_person_refusal() {
        assert_eq!(
            request("「先に読んどいてくれない？」", "먼저 읽어보세요."),
            "먼저 읽어 놔 줄래."
        );
        for source in [
            "「彼は読んでくれない？」",
            "「読んでくれないのはなぜ？」",
            "「読まないでくれない？」",
        ] {
            assert_eq!(request(source, "읽어주세요."), "읽어주세요.");
        }
    }
}
