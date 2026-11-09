//! Explicit, opt-in model integration test. Ordinary CI never downloads weights.
#![cfg(feature = "offline-model")]
use syousetu_translate::{
    learning::Memory,
    offline::{OfflineTranslator, ParallelPolicy, VocabularyPolicy},
};

#[test]
#[ignore = "requires pinned assets in SYOUSETU_MODEL_DIR; inference can run without networking"]
fn direct_translation_parallel_merge_and_user_corrections() {
    let root = syousetu_translate::models::default_dir();
    let input = [
        "私は学生です。",
        "今日は雨が降っています。",
        "彼女は図書館で本を読んでいます。",
        "",
        "ASCII 123",
        "私は学生です。",
    ]
    .map(str::to_owned);
    let mut serial = OfflineTranslator::open(&root, Memory::default(), 1).unwrap();
    let begin = std::time::Instant::now();
    let expected = serial.translate_lines(&input).unwrap();
    let serial_seconds = begin.elapsed().as_secs_f64();
    for text in &expected[..3] {
        assert!(
            text.chars().any(|c| ('가'..='힣').contains(&c)),
            "Expected Korean: {text}"
        );
    }
    assert_eq!(expected[3], "");
    assert_eq!(expected[4], "ASCII 123");
    assert_eq!(expected[0], expected[5]);
    drop(serial);
    let mut parallel =
        OfflineTranslator::open_with_policy(&root, Memory::default(), 4, ParallelPolicy::Parallel)
            .unwrap();
    let begin = std::time::Instant::now();
    let actual = parallel.translate_lines(&input).unwrap();
    let parallel_seconds = begin.elapsed().as_secs_f64();
    assert_eq!(
        actual, expected,
        "Parallel workers must preserve exact order and content"
    );
    assert_eq!(
        parallel.translate(&input[1]).unwrap(),
        expected[1],
        "KV cache cannot leak across sentences"
    );
    drop(parallel);
    let mut memory = Memory::default();
    memory
        .learn(&input[0], "저는 학생입니다. (사용자 교정)")
        .unwrap();
    memory
        .learn_term("図書館", &expected[2], "도서관 문장 교정")
        .unwrap();
    let mut learned = OfflineTranslator::open(&root, memory, 2).unwrap();
    assert_eq!(
        learned.translate(&input[0]).unwrap(),
        "저는 학생입니다. (사용자 교정)"
    );
    assert_eq!(learned.translate(&input[2]).unwrap(), "도서관 문장 교정");
    println!(
        "{}",
        serde_json::json!({"serial_seconds":serial_seconds,"parallel_seconds":parallel_seconds,"outputs":expected})
    );
}

#[test]
#[ignore = "requires pinned local model assets; opt-in lexical mode changes full-vocabulary output"]
fn shortlist_parallel_agrees_with_serial_and_scope_changes_do_not_reuse_stale_results() {
    let root = syousetu_translate::models::default_dir();
    let input = [
        "私は学生です。",
        "今日は雨が降っています。",
        "Alexandraは図書館で本を読んでいます。",
        "猫が庭を歩いています。",
        "ASCII 123",
        "",
        "私は学生です。",
    ]
    .map(str::to_owned);
    let build = |workers, policy| {
        OfflineTranslator::builder(&root)
            .memory(Memory::default())
            .workers(workers)
            .parallel_policy(policy)
            .vocabulary_policy(VocabularyPolicy::KoreanShortlist)
            .build()
            .unwrap()
    };
    let mut serial = build(1, ParallelPolicy::Serial);
    let expected = serial.translate_lines(&input).unwrap();
    drop(serial);
    let mut parallel = build(32, ParallelPolicy::Parallel);
    let actual = parallel.translate_lines(&input).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual[0], actual[6]);
    assert_eq!(actual[4], "ASCII 123");
    assert_eq!(actual[5], "");
    parallel
        .translate_lines(&["Christopherは駅に到着しました。".to_owned()])
        .unwrap();
    assert_eq!(parallel.translate_lines(&input).unwrap(), expected);
}

#[test]
#[ignore = "requires pinned local model assets; validates continuous refill and ordered streaming"]
fn variable_length_contexts_refill_and_stream_in_order_at_16_32_64() {
    let root = syousetu_translate::models::default_dir();
    let input = [
        "私は学生です。",
        "今日は雨が降っています。",
        "彼女は図書館で本を読んでいます。",
        "朝、窓を開けると冷たい風が部屋に入りました。",
        "猫が庭を歩いています。",
        "少年は妹に傘を渡しました。",
        "母は台所で夕食を作っています。",
        "父は毎朝新聞を読みます。",
        "友人と駅で待ち合わせをしました。",
        "新しい仕事を始める前に、必要な資料を確認しました。",
        "部屋の明かりを消してください。",
        "道に迷ったので、近くの店で尋ねました。",
        "海辺の小さな村で、友人が私を待っていました。",
        "彼は少し考えてから答えました。",
        "古い扉の向こうから誰かの声が聞こえました。",
        "先生は難しい問題を説明しました。",
        "雨が降り始めたので、急いで家に帰りました。",
        "子供たちは公園で遊んでいます。",
        "荷物をここに置いてください。",
        "明日、もう一度ここに来ます。",
        "日本語の文です。\n\n次の文です。",
        "",
        "ASCII 123 😀",
        "私は学生です。",
    ]
    .map(str::to_owned);
    let mut serial = OfflineTranslator::open(&root, Memory::default(), 1).unwrap();
    let expected = serial.translate_lines(&input).unwrap();
    drop(serial);
    for workers in [2, 16, 32, 64] {
        let mut engine = OfflineTranslator::open_with_policy(
            &root,
            Memory::default(),
            workers,
            ParallelPolicy::Parallel,
        )
        .unwrap();
        let mut next = 0;
        engine
            .translate_lines_to(&input, |_, _| {}, &mut |index, text| {
                assert_eq!(index, next);
                assert_eq!(text, expected[index]);
                next += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(next, input.len());
    }
}
