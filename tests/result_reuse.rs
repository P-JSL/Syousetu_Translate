#![cfg(feature = "offline-model")]
use syousetu_translate::{
    learning::Memory,
    offline::{OfflineTranslator, ParallelPolicy},
};

#[test]
#[ignore = "requires the pinned local model; checks a fresh process-equivalent cache and correction invalidation"]
fn completed_results_are_identical_and_never_override_new_user_corrections() {
    let root = syousetu_translate::models::default_dir();
    let directory = std::env::temp_dir().join(format!(
        "syousetu-model-result-cache-{}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let input = [
        "私は学生です。",
        "今日は雨が降っています。",
        "彼女は図書館で本を読んでいます。",
        "私は学生です。",
        "",
        "ASCII 123",
    ]
    .map(str::to_owned);
    let open = |memory| {
        OfflineTranslator::builder(&root)
            .memory(memory)
            .workers(4)
            .parallel_policy(ParallelPolicy::Parallel)
            .translation_cache(&directory)
            .build()
            .unwrap()
    };
    let mut first = open(Memory::default());
    let expected = first.translate_lines(&input).unwrap();
    first.flush_result_cache().unwrap();
    assert!(first.result_cache_stats().miss_lookups > 0);
    drop(first);
    let mut second = open(Memory::default());
    assert_eq!(second.result_cache_stats().restored_entries, 3);
    assert_eq!(second.translate_lines(&input).unwrap(), expected);
    assert_eq!(second.result_cache_stats().miss_lookups, 0);
    drop(second);
    let mut memory = Memory::default();
    memory
        .learn(&input[0], "이 문장은 사용자가 직접 고쳤습니다.")
        .unwrap();
    let mut changed = open(memory);
    assert_eq!(changed.result_cache_stats().restored_entries, 0);
    assert_eq!(
        changed.translate(&input[0]).unwrap(),
        "이 문장은 사용자가 직접 고쳤습니다."
    );
    drop(changed);
    assert!(OfflineTranslator::builder(&directory.join("missing-model"))
        .translation_cache(&directory)
        .build()
        .is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
