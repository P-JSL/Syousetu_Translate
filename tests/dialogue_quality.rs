//! Authored cases absent from the bundled whole-sentence corrections. This
//! checks specific semantic regressions, not a claim of general fluency.
#![cfg(feature = "offline-model")]
use syousetu_translate::{
    learning::Memory,
    offline::{OfflineTranslator, ParallelPolicy},
};
#[test]
#[ignore = "requires the pinned local model; no network or bundled corrections"]
fn unseen_contractions_keep_obligation_negation_aspect_and_speech_level() {
    let sources = [
        "「そんなこと、俺は知らねえよ！」",
        "「もう全部食べちゃったんだよ……」",
        "「何も聞いてないんだよ！」",
        "「明日までに読んどいてくれない？」",
        "「そんなに急がなくちゃいけないの？」",
        "「本当に、あなたが来てくれて嬉しいです。」",
        "「ねえ、そんな顔しないでよ。」",
    ]
    .map(str::to_owned);
    let bundled = Memory::bundled().unwrap();
    assert!(sources
        .iter()
        .all(|source| bundled.resolve(source).is_none()));
    let mut engine = OfflineTranslator::open_with_policy(
        &syousetu_translate::models::default_dir(),
        Memory::default(),
        32,
        ParallelPolicy::Parallel,
    )
    .unwrap();
    let outputs = engine.translate_lines(&sources).unwrap();
    assert!(
        outputs[0].contains("모르") || outputs[0].contains("몰라"),
        "knowledge negation: {}",
        outputs[0]
    );
    assert!(outputs[1].contains("먹어버렸어") && !outputs[1].contains("먹어봤"));
    assert!(outputs[2].contains("못") || outputs[2].contains("않"));
    assert!(outputs[3].contains("읽어") && outputs[3].contains("줄래"));
    assert!(outputs[4].contains("서둘러야") && !outputs[4].contains("않아도"));
    assert!(outputs[5].contains("기뻐요"));
    assert!(outputs[6].contains("하지 마"));
    for output in outputs {
        assert!(output.starts_with('“') && output.ends_with('”'));
    }
}
