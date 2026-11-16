//! Model-independent language-processing checks, including the legacy path.
use anyhow::{bail, Result};
use syousetu_translate::learning::Memory;
use syousetu_translate::translation::{
    engine::{Passthrough, Translator},
    filter::{Dictionary, Pipeline},
    prose,
    session::Session,
};

struct Reply(&'static str);
impl Translator for Reply {
    fn translate(&mut self, source: &str) -> Result<String> {
        if source == "故障" {
            bail!("injected translation failure");
        }
        Ok(self.0.to_owned())
    }
}

#[test]
fn both_pipeline_profiles_share_source_qualified_dialogue_processing() {
    for pipeline in [
        Pipeline::new(Dictionary::empty()),
        Pipeline::legacy(Dictionary::empty()),
    ] {
        let pipeline = pipeline.with_natural_text();
        assert_eq!(
            pipeline
                .translate(&mut Reply("기다려 ."), "「待って！」")
                .unwrap(),
            "“기다려!”"
        );
        assert_eq!(
            pipeline
                .translate(
                    &mut Reply("우리 영광스럽게도, 당신의 명령을 기다리고 있습니다."),
                    "「陛下、ご命令をお待ちしております。」"
                )
                .unwrap(),
            "“폐하, 당신의 명령을 기다리고 있습니다.”"
        );
        struct Obligation;
        impl Translator for Obligation {
            fn translate(&mut self, source: &str) -> Result<String> {
                assert_eq!(source, "「そんなに急ぐ必要があるの?」");
                Ok("그렇게 서둘러야 합니까?".into())
            }
        }
        assert_eq!(
            pipeline
                .translate(&mut Obligation, "「そんなに急がなくちゃいけないの？」")
                .unwrap(),
            "“그렇게 서둘러야 해?”"
        );
    }
}

#[test]
fn raw_pipeline_and_explicit_dictionary_choices_keep_their_output() {
    assert_eq!(
        Pipeline::new(Dictionary::empty())
            .translate(&mut Reply("기다려 ."), "「待って！」")
            .unwrap(),
        "기다려 ."
    );
    let dictionary = Dictionary::parse("猫?\t직접  교정 입니다 .\n").unwrap();
    let pipeline = Pipeline::new(dictionary).with_natural_text();
    assert_eq!(
        pipeline.translate(&mut Passthrough, "猫？").unwrap(),
        "직접  교정 입니다 ."
    );
    let dictionary = Dictionary::parse("知らない\t모르  겠어요 .\n").unwrap();
    assert_eq!(
        Pipeline::new(dictionary)
            .with_natural_text()
            .translate(&mut Passthrough, "知らねえ")
            .unwrap(),
        "모르  겠어요 ."
    );
}

#[test]
fn reviewed_corrections_are_shared_without_overriding_a_manual_dictionary() {
    for pipeline in [
        Pipeline::new(Dictionary::empty()),
        Pipeline::legacy(Dictionary::empty()),
    ] {
        let mut memory = Memory::default();
        memory.learn("急がないで。", "서두르지  말아요 .").unwrap();
        memory.learn_term("東京", "도쿄", "토쿄").unwrap();
        let pipeline = pipeline.with_natural_text().with_memory(memory);
        assert_eq!(
            pipeline
                .translate(&mut Reply("unused"), "「急がないで。」")
                .unwrap(),
            "“서두르지  말아요 .”"
        );
        assert_eq!(
            pipeline
                .translate(&mut Reply("도쿄로 간다."), "東京へ行く。")
                .unwrap(),
            "토쿄로 간다."
        );
        assert_eq!(
            pipeline
                .translate(&mut Reply("도쿄로 간다."), "京都へ行く。")
                .unwrap(),
            "도쿄로 간다."
        );
    }
    let mut memory = Memory::default();
    memory.learn("猫？", "다른 교정").unwrap();
    let dictionary = Dictionary::parse("猫?\t직접  지정 .\n").unwrap();
    assert_eq!(
        Pipeline::new(dictionary)
            .with_natural_text()
            .with_memory(memory)
            .translate(&mut Passthrough, "猫？")
            .unwrap(),
        "직접  지정 ."
    );
}

#[test]
fn paragraph_and_crlf_boundaries_are_not_guessed_or_collapsed() {
    assert_eq!(
        prose::finish_lines(
            "「待って！」\n\n説明。",
            "기다려 .\r\n\r\n그녀는  문을 열었다.\r"
        ),
        "“기다려!”\r\n\r\n그녀는 문을 열었다.\r"
    );
    for output in [
        "기다려 .\n설명입니다.",
        "기다려 .\n끼어든 문장\n설명입니다.",
    ] {
        assert_eq!(
            prose::finish_lines("「待って！」\n\n説明。", output),
            output
        );
    }
    assert_eq!(
        prose::finish_lines("이미  한국어 입니다 .", "이미  한국어 입니다 ."),
        "이미  한국어 입니다 ."
    );
    assert_eq!(prose::finish_lines("ASCII 123", "ASCII 123"), "ASCII 123");
}

#[test]
fn normalized_results_still_stream_only_the_completed_prefix_on_failure() {
    let mut session = Session::local(
        Pipeline::new(Dictionary::empty()).with_natural_text(),
        Box::new(Reply("기다려 .")),
    );
    let mut emitted = Vec::new();
    let sources = ["「待って！」", "故障"].map(str::to_owned);
    assert!(session
        .translate_lines_into(&sources, &mut |index, text| {
            emitted.push((index, text.to_owned()));
            Ok(())
        })
        .is_err());
    assert_eq!(emitted, [(0, "“기다려!”".into())]);
}
