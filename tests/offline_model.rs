//! Explicit, opt-in model integration test. Ordinary CI never downloads weights.
#![cfg(feature = "offline-model")]
use syousetu_translate::{
    learning::Memory,
    offline::{OfflineTranslator, ParallelPolicy},
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
