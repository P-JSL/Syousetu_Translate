//! Small, source-qualified aspect and speech-act corrections. Never change an arbitrary
//! Korean "아/어 봤다" (which may genuinely describe an experience).
use super::Utterance;
use std::borrow::Cow;

pub(super) fn request<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let source = utterance.inner;
    if !super::japanese::friendly_request(utterance)
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

pub(super) fn completion<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let source = utterance.inner;
    if utterance.quotes.is_none()
        || source.contains("てみ")
        || source.contains("でみ")
        || source.contains("試し")
        || source.contains("ことが")
        || source.contains("ことは")
        || utterance.inner.contains(['「', '」', '『', '』'])
        || target.contains(['“', '”', '‘', '’', '「', '」'])
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
        ("読んじゃった", "読んでしまった", "읽어봤", "읽어버렸"),
        ("読んじゃった", "読んでしまった", "읽어보았", "읽어버렸"),
        ("話しちゃった", "話してしまった", "말해봤", "말해버렸"),
        ("話しちゃった", "話してしまった", "말해보았", "말해버렸"),
        ("話しちゃった", "話してしまった", "얘기해 봤", "얘기해 버렸"),
        (
            "話しちゃった",
            "話してしまった",
            "얘기해 보았",
            "얘기해 버렸",
        ),
        ("聞いちゃった", "聞いてしまった", "들어봤", "들어버렸"),
    ] {
        if (source.contains(contraction) || source.contains(expanded)) && target.contains(wrong) {
            return Cow::Owned(target.replace(wrong, corrected));
        }
    }
    Cow::Borrowed(target)
}

pub(super) fn formal_apology<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let source = utterance
        .inner
        .trim_end_matches(['。', '！', '!', '…', '.']);
    let target_body = target.trim_end_matches(['。', '.', '!', '！', '…']);
    if utterance.inner.contains(['「', '」', '『', '』'])
        || target.contains(['“', '”', '‘', '’', '「', '」'])
    {
        return Cow::Borrowed(target);
    }
    for (source_ending, previous, corrected) in [
        ("申し訳ございませんでした", "미안했어요", "죄송했습니다"),
        ("申し訳ございませんでした", "미안했습니다", "죄송했습니다"),
        ("申し訳ございません", "미안해요", "죄송합니다"),
        ("申し訳ございません", "미안합니다", "죄송합니다"),
    ] {
        if source.ends_with(source_ending) {
            if let Some(stem) = target_body.strip_suffix(previous) {
                return Cow::Owned(format!("{stem}{corrected}{}", &target[target_body.len()..]));
            }
        }
    }
    Cow::Borrowed(target)
}

#[cfg(test)]
mod tests {
    use super::{Cow, Utterance};

    fn completion<'a>(source: &str, target: &'a str) -> Cow<'a, str> {
        super::completion(Utterance::parse(source), target)
    }

    fn formal_apology<'a>(source: &str, target: &'a str) -> Cow<'a, str> {
        super::formal_apology(Utterance::parse(source), target)
    }

    fn request<'a>(source: &str, target: &'a str) -> Cow<'a, str> {
        super::request(Utterance::parse(source), target)
    }
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
            "「『食べちゃった』という表現です」",
            "「読んでみてから、食べちゃったんだよ」",
        ] {
            assert_eq!(completion(source, "한번 먹어봤어."), "한번 먹어봤어.");
        }
        let mixed = "先に読んじゃったけど、前にも読んだことがある。";
        assert_eq!(
            completion(&format!("「{mixed}」"), "먼저 읽어봤고, 예전에도 읽어봤어."),
            "먼저 읽어봤고, 예전에도 읽어봤어."
        );
        assert_eq!(
            completion("「もう全部読んじゃったんだよ」", "이제 다 읽어봤어."),
            "이제 다 읽어버렸어."
        );
        assert_eq!(
            completion("「つい話しちゃった。」", "그만 얘기해 보았어."),
            "그만 얘기해 버렸어."
        );
    }
    #[test]
    fn explicit_formal_apology_keeps_past_tense_and_is_not_a_quoted_or_negated_apology() {
        assert_eq!(
            formal_apology("「陛下、申し訳ございません。」", "폐하, 미안해요."),
            "폐하, 죄송합니다."
        );
        assert_eq!(
            formal_apology("「本当に申し訳ございませんでした。」", "정말 미안했어요."),
            "정말 죄송했습니다."
        );
        for source in [
            "「ごめんね。」",
            "「申し訳ございませんとは言わない。」",
            "「『申し訳ございません』という言葉。」",
        ] {
            assert_eq!(formal_apology(source, "미안해요."), "미안해요.");
        }
        assert_eq!(
            formal_apology("「申し訳ございません。」", "미안하지 않아요."),
            "미안하지 않아요."
        );
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
