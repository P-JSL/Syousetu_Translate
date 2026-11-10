//! Expand only reviewed contractions with unambiguous morphology. The original
//! source remains available for register, pauses, quoted output and corrections.
use std::{borrow::Cow, sync::LazyLock};
mod intent;
pub(super) fn friendly_request(source: &str) -> bool {
    intent::request_stem(source).is_some()
}
const RULES: &[(&str, &str)] = &[
    ("知らねえ", "知らない"),
    ("知らねぇ", "知らない"),
    ("知んない", "知らない"),
    ("分かんねえ", "分からない"),
    ("わかんねえ", "わからない"),
    ("できねえ", "できない"),
    ("出来ねえ", "出来ない"),
    ("すげえ", "すごい"),
    ("すげぇ", "すごい"),
    ("うるせえ", "うるさい"),
    ("うるせぇ", "うるさい"),
    ("なきゃいけない", "なければいけない"),
    ("なきゃならない", "なければならない"),
    ("なくちゃいけない", "なくてはいけない"),
    ("なくちゃならない", "なくてはならない"),
    ("なきゃダメ", "なければだめ"),
    ("なくちゃダメ", "なくてはだめ"),
    ("行っちゃった", "行ってしまった"),
    ("行っちゃう", "行ってしまう"),
    ("やっちゃった", "やってしまった"),
    ("やっちゃう", "やってしまう"),
    ("食べちゃった", "食べてしまった"),
    ("食べちゃう", "食べてしまう"),
    ("飲んじゃった", "飲んでしまった"),
    ("飲んじゃう", "飲んでしまう"),
    ("買っちゃった", "買ってしまった"),
    ("寝ちゃった", "寝てしまった"),
    ("忘れちゃった", "忘れてしまった"),
    ("終わっちゃった", "終わってしまった"),
    ("読んどいて", "読んでおいて"),
    ("やっといて", "やっておいて"),
    ("分かんない", "分からない"),
    ("わかんない", "わからない"),
    ("なんつーか", "なんというか"),
    ("っていうか", "というか"),
    ("待ってる", "待っている"),
    ("やってる", "やっている"),
    ("見てる", "見ている"),
    ("聞いてる", "聞いている"),
    ("読んでる", "読んでいる"),
    ("言ってる", "言っている"),
    ("思ってる", "思っている"),
    ("話してる", "話している"),
    ("泣いてる", "泣いている"),
    ("笑ってる", "笑っている"),
    ("寝てる", "寝ている"),
    ("知ってる", "知っている"),
    ("持ってる", "持っている"),
    ("立ってる", "立っている"),
    ("見てない", "見ていない"),
    ("聞いてない", "聞いていない"),
    ("やってない", "やっていない"),
    ("読んでない", "読んでいない"),
    ("寝てない", "寝ていない"),
];
pub fn expand(source: &str) -> Cow<'_, str> {
    static MATCHER: LazyLock<aho_corasick::AhoCorasick> = LazyLock::new(|| {
        aho_corasick::AhoCorasickBuilder::new()
            .match_kind(aho_corasick::MatchKind::LeftmostLongest)
            .build(RULES.iter().map(|(old, _)| old))
            .expect("static reviewed contractions")
    });
    if !MATCHER.is_match(source) {
        return intent::canonicalize(Cow::Borrowed(source));
    }
    intent::canonicalize(Cow::Owned(MATCHER.replace_all(
        source,
        &RULES.iter().map(|(_, new)| *new).collect::<Vec<_>>(),
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obligation_completion_and_preparation_keep_their_distinct_meanings() {
        assert_eq!(expand("薬を飲まなきゃいけない。"), "薬を飲む必要がある。");
        assert_eq!(
            expand("もう食べちゃったんだよ。"),
            "もう食べてしまったんだよ。"
        );
        assert_eq!(expand("先に読んどいて。"), "先に読んでおいて。");
        assert_eq!(expand("何も聞いてないんだよ。"), "何も聞いていないんだよ。");
        assert_eq!(expand("本を読んでるよ。"), "本を読んでいるよ。");
    }
    #[test]
    fn regular_verbs_conditionals_and_ambiguous_slang_are_never_blanket_rewritten() {
        for source in [
            "捨てる。",
            "当てる。",
            "勝てる。",
            "立てる。",
            "ホテルへ行く。",
            "君が行かなきゃ、困る。",
            "草原",
            "庭の草",
            "やばい",
            "いつか会える。",
            "それちゃう。",
        ] {
            assert_eq!(expand(source), source);
        }
    }
}
