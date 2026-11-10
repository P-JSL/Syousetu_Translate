//! Canonical forms for explicitly stated obligations and friendly requests.
//! Bare negative conditionals and third-person refusals are left alone.
use super::super::Utterance;
use std::{borrow::Cow, sync::LazyLock};

const VERBS: &[(&str, &str)] = &[
    ("行か", "行く"),
    ("帰ら", "帰る"),
    ("急が", "急ぐ"),
    ("書か", "書く"),
    ("聞か", "聞く"),
    ("読ま", "読む"),
    ("飲ま", "飲む"),
    ("待た", "待つ"),
    ("持た", "持つ"),
    ("立た", "立つ"),
    ("入ら", "入る"),
    ("作ら", "作る"),
    ("使わ", "使う"),
    ("話さ", "話す"),
    ("探さ", "探す"),
    ("選ば", "選ぶ"),
    ("休ま", "休む"),
    ("泳が", "泳ぐ"),
    ("学ば", "学ぶ"),
    ("遊ば", "遊ぶ"),
    ("働か", "働く"),
    ("知ら", "知る"),
    ("起き", "起きる"),
    ("食べ", "食べる"),
    ("見", "見る"),
    ("寝", "寝る"),
    ("着", "着る"),
    ("開け", "開ける"),
    ("閉め", "閉める"),
    ("始め", "始める"),
    ("止め", "止める"),
    ("忘れ", "忘れる"),
    ("調べ", "調べる"),
    ("受け", "受ける"),
    ("捨て", "捨てる"),
    ("答え", "答える"),
    ("し", "する"),
    ("来", "来る"),
];
pub(super) fn request_stem(source: &str) -> Option<&str> {
    let utterance = Utterance::parse(source);
    if !source.trim().starts_with(['「', '『'])
        || utterance.quotes.is_none()
        || ["は", "が", "なぜ", "どうして", "くれないのは"]
            .iter()
            .any(|part| utterance.inner.contains(part))
    {
        return None;
    }
    for ending in ["くれない？", "くれない?", "くれないか？", "くれないか?"] {
        if let Some(stem) = utterance.inner.strip_suffix(ending) {
            if stem.ends_with(['て', 'で']) {
                return Some(stem);
            }
        }
    }
    None
}
pub(super) fn canonicalize(source: Cow<'_, str>) -> Cow<'_, str> {
    if [
        "という表現",
        "という言葉",
        "わけではない",
        "わけじゃない",
        "ことはない",
        "とは限らない",
    ]
    .iter()
    .any(|part| source.contains(part))
    {
        return source;
    }
    let mut output = source;
    if output.contains("なければ") || output.contains("なくては") {
        static OBLIGATIONS: LazyLock<(aho_corasick::AhoCorasick, Vec<String>)> =
            LazyLock::new(|| {
                let mut patterns = Vec::new();
                let mut replacements = Vec::new();
                for &(stem, verb) in VERBS {
                    for ending in [
                        "なければいけない",
                        "なければならない",
                        "なくてはいけない",
                        "なくてはならない",
                        "なければだめ",
                        "なくてはだめ",
                    ] {
                        patterns.push(format!("{stem}{ending}"));
                        replacements.push(format!("{verb}必要がある"));
                    }
                }
                (
                    aho_corasick::AhoCorasickBuilder::new()
                        .match_kind(aho_corasick::MatchKind::LeftmostLongest)
                        .build(patterns)
                        .expect("reviewed obligation forms"),
                    replacements,
                )
            });
        if OBLIGATIONS.0.is_match(output.as_ref()) {
            output = Cow::Owned(OBLIGATIONS.0.replace_all(&output, &OBLIGATIONS.1));
        }
    }
    // A pure quotation ending in Vて/で + くれない？ can be a friendly
    // request. Do not reinterpret a stated third-person refusal as a request.
    if let Some(stem) = request_stem(&output) {
        let open = output.trim().chars().next().unwrap();
        let close = output.trim().chars().next_back().unwrap();
        return Cow::Owned(format!("{open}{stem}ください。{close}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_obligation_stays_positive_and_refusal_stays_negative() {
        assert_eq!(
            canonicalize(Cow::Borrowed("「急がなくてはいけないの？」")),
            "「急ぐ必要があるの？」"
        );
        assert_eq!(
            canonicalize(Cow::Borrowed("薬を飲まなければならない。")),
            "薬を飲む必要がある。"
        );
        for text in [
            "行かなければ、会えない。",
            "行かなければいけないわけではない。",
            "「彼は読んでくれない？」",
            "「読んでくれないのはなぜ？」",
        ] {
            assert_eq!(canonicalize(Cow::Borrowed(text)), text);
        }
        assert_eq!(
            canonicalize(Cow::Borrowed("「先に読んでおいてくれない？」")),
            "「先に読んでおいてください。」"
        );
    }
}
