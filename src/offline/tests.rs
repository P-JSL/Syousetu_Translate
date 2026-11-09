use super::{
    runner::generation::repeated_trigram_tokens, text::split_boundary, tokenizer::Tokenizer,
};
use crate::models;

#[test]
fn repetition_guard_blocks_only_previously_completed_trigrams() {
    assert!(repeated_trigram_tokens(&[1, 2]).is_empty());
    assert_eq!(repeated_trigram_tokens(&[1, 2, 3, 4, 1, 2]), [3].into());
    assert!(repeated_trigram_tokens(&[1, 2, 3, 4, 2, 1]).is_empty());
}
#[test]
#[ignore = "requires pinned model assets; token ids are from official Transformers v4.46.3"]
fn tokenization_matches_official_small100_language_prefix_and_bpe_ids() {
    let tokenizer = Tokenizer::open(&models::default_dir().join("ja-ko")).unwrap();
    assert_eq!(tokenizer.ko, 128052);
    assert_eq!(
        tokenizer.encode("私は学生です。").unwrap(),
        [77828, 13261, 2357, 37]
    );
    assert_eq!(
        tokenizer.encode("今日は雨が降っています。").unwrap(),
        [20511, 30299, 26062, 413, 36393, 23654, 37]
    );
    assert_eq!(
        tokenizer.encode("こんにちは。").unwrap(),
        [22, 13552, 8042, 474, 14309, 545, 37]
    );
}
#[test]
fn unicode_splits_preserve_every_character() {
    for s in [
        "日本語の長い文です。次の文です。",
        "😀😀あいうえお",
        "abc def ghi",
        "長文",
    ] {
        let b = split_boundary(s).unwrap();
        assert!(b > 0 && b < s.len() && s.is_char_boundary(b));
        assert_eq!(format!("{}{}", &s[..b], &s[b..]), s);
    }
    assert_eq!(split_boundary("あ"), None);
}
