use super::{particles::choose_josa, text::hiragana_to_hangul};

#[test]
fn java_dictionary_readings_bom_and_extra_columns() {
    let dictionary =
        super::Dictionary::parse("\u{feff}カナ#N\t가나\textra\n未知漢字\nきゃ\n").unwrap();
    assert_eq!(dictionary.entries.len(), 2);
    assert!(dictionary
        .entries
        .iter()
        .any(|e| e.source == "カナ" && e.target == "가나" && e.name));
    assert!(dictionary
        .entries
        .iter()
        .any(|e| e.source == "きゃ" && e.target == "캬"));
}
#[test]
fn literal_placeholder_strings_are_not_overwritten() {
    let pipeline = super::Pipeline::new(super::Dictionary::parse("猫\t고양이").unwrap());
    let mut engine = crate::translation::engine::Passthrough;
    assert_eq!(
        pipeline
            .translate(&mut engine, "c$0$c 猫 s$0$s 😀")
            .unwrap(),
        "c$0$c 고양이 s$0$s 😀"
    );
    assert!(super::Dictionary::parse("っ\n").unwrap().entries.is_empty());
}
#[test]
fn legacy_annotations_removed_before_protected_values_return() {
    struct Annotated;
    impl crate::translation::engine::Translator for Annotated {
        fn translate(&mut self, text: &str) -> anyhow::Result<String> {
            Ok(format!("{text}[annotation](note)"))
        }
    }
    let pipeline = super::Pipeline::legacy(super::Dictionary::parse("猫\t고양이[이름]").unwrap());
    assert_eq!(
        pipeline.translate(&mut Annotated, "猫 (犬)").unwrap(),
        "고양이[이름] (犬)"
    );
}
#[test]
fn fullwidth_brackets_are_protected_after_punctuation_normalization() {
    struct Annotated;
    impl crate::translation::engine::Translator for Annotated {
        fn translate(&mut self, text: &str) -> anyhow::Result<String> {
            Ok(format!("{text}[annotation]"))
        }
    }
    let pipeline = super::Pipeline::legacy(super::Dictionary::parse("猫\t고양이").unwrap());
    assert_eq!(
        pipeline
            .translate(&mut Annotated, "こんにちは（猫）？")
            .unwrap(),
        "こんにちは(고양이)?"
    );
}
use super::*;
use crate::translation::engine::Passthrough;
#[test]
fn dictionary_overlaps() {
    let dict = Dictionary::parse("天\t하늘\n天気\t날씨\n").unwrap();
    let pipe = Pipeline::new(dict);
    assert_eq!(
        pipe.translate(&mut Passthrough, "天気はいい").unwrap(),
        "날씨はいい"
    );
}
#[test]
fn unicode_and_josa() {
    assert_eq!(choose_josa(Some('길'), "으로"), "로");
    assert_eq!(choose_josa(Some('집'), "으로"), "으로");
    assert_eq!(choose_josa(Some('사'), "은"), "는");
    assert_eq!(hiragana_to_hangul("きゃ"), "캬");
}
#[cfg(feature = "legacy")]
#[test]
fn native_dictionary_names_keep_korean_josa() {
    use crate::translation::engine::{create_engine, EngineConfig, EngineMode};
    let custom = Dictionary::parse("猫\t사자#N\n").unwrap();
    let pipeline = Pipeline::new(custom);
    let mut native = create_engine(&EngineConfig {
        mode: EngineMode::Native,
        eztrans_dir: None,
        native_dictionary: None,
        strict_native: false,
    })
    .unwrap();
    assert_eq!(
        pipeline.translate(&mut *native, "猫は学生です。").unwrap(),
        "사자는 학생입니다。"
    );
}
#[test]
fn preserve_emoji_and_brackets() {
    let pipe = Pipeline::new(Dictionary::empty());
    let result = pipe
        .translate(&mut Passthrough, "こんにちは 😀 (世界)")
        .unwrap();
    assert_eq!(result, "こんにちは 😀 (世界)");
}
