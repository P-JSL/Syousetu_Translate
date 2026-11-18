//! Authored cases absent from the bundled whole-sentence corrections. This
//! checks specific semantic regressions, not a claim of general fluency.
#![cfg(feature = "offline-model")]
use syousetu_translate::{
    learning::Memory,
    offline::{InferenceDevice, OfflineTranslator, ParallelPolicy},
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

#[test]
#[ignore = "requires the pinned local model; prohibitions are not correction-memory answers"]
fn explicit_prohibitions_do_not_become_permission_or_a_double_negative() {
    let sources = [
        "「その水は飲んじゃだめだよ。」",
        "「まだ中に入っちゃだめだよ。」",
        "「もう食べちゃだめなの？」",
    ]
    .map(str::to_owned);
    let memory = Memory::bundled().unwrap();
    assert!(sources
        .iter()
        .all(|source| memory.resolve(source).is_none()));
    let mut engine = OfflineTranslator::open_with_policy(
        &syousetu_translate::models::default_dir(),
        Memory::default(),
        32,
        ParallelPolicy::Parallel,
    )
    .unwrap();
    let output = engine.translate_lines(&sources).unwrap();
    assert!(
        output[0].contains("마시지 말") && !output[0].contains("않아도"),
        "{}",
        output[0]
    );
    assert!(
        output[1].contains("들어가지 마") && !output[1].contains("안 들어가지"),
        "{}",
        output[1]
    );
    assert!(
        (output[2].contains("안 돼") || output[2].contains("안돼")) && output[2].contains('?'),
        "{}",
        output[2]
    );
}

#[test]
#[ignore = "requires the pinned local model; tests dynamically aligned model output"]
fn explicit_speech_acts_and_mixed_register_survive_inference() {
    let sources = [
        "「今は大丈夫です。次は私がやるよ。」",
        "「今日はその本を読まなくてもいいよ。」",
        "「もう中に入ってもいいですか？」",
        "「この水を飲まなきゃだめです。」",
        "「いいえ、まだ中に入っちゃダメです。」",
        "「灯りを消さないでよ」と、彼女は小さな声で言った。",
    ]
    .map(str::to_owned);
    let model_dir = syousetu_translate::models::default_dir();
    let open = |workers, policy| {
        OfflineTranslator::builder(&model_dir)
            .memory(Memory::default())
            .workers(workers)
            .parallel_policy(policy)
            .device(InferenceDevice::Cpu)
            .build()
            .unwrap()
    };
    let mut engine = open(32, ParallelPolicy::Parallel);
    let outputs = engine.translate_lines(&sources).unwrap();
    for (output, required) in outputs.iter().zip([
        "괜찮아요.",
        "읽지 않아도 돼",
        "들어가도 될까요?",
        "마셔야 해요",
        "들어가지 마세요",
        "끄지 마”라고",
    ]) {
        assert!(output.contains(required), "{required}: {output}");
    }
    assert!(!outputs[4].contains("안 들어가지"), "{}", outputs[4]);
    drop(engine);
    let mut sequential = open(1, ParallelPolicy::Serial);
    assert_eq!(sequential.translate_lines(&sources).unwrap(), outputs);
}
