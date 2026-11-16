use super::*;
use std::{fs, path::PathBuf, sync::atomic::AtomicUsize};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "syousetu-results-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> disk::Store {
        disk::Store::new(&self.0, "a".repeat(64)).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn completed_results_survive_an_error_and_a_new_instance() {
    let directory = Directory::new();
    let result: Result<()> = (|| {
        let mut cache = Cache::with_store(directory.store())?;
        cache.insert("完成した文", "완료한 문장".into());
        anyhow::bail!("later model/output failure")
    })();
    assert!(result.is_err());
    let cache = Cache::with_store(directory.store()).unwrap();
    assert_eq!(cache.get("完成した文").as_deref(), Some("완료한 문장"));
    assert!(cache.get("未完了の文").is_none());
    assert_eq!(cache.stats().restored_entries, 1);
}
#[test]
fn stale_instances_merge_only_completed_deltas_without_losing_other_updates() {
    let directory = Directory::new();
    let mut first = Cache::with_store(directory.store()).unwrap();
    let mut second = Cache::with_store(directory.store()).unwrap();
    first.insert("最初", "처음".into());
    second.insert("次", "다음".into());
    first.flush().unwrap();
    second.flush().unwrap();
    let saved = Cache::with_store(directory.store()).unwrap();
    assert_eq!(saved.get("最初").as_deref(), Some("처음"));
    assert_eq!(saved.get("次").as_deref(), Some("다음"));
}
#[test]
fn a_busy_writer_keeps_pending_results_for_a_later_retry() {
    use fs2::FileExt;
    let directory = Directory::new();
    let mut cache = Cache::with_store(directory.store()).unwrap();
    cache.insert("原文", "번역".into());
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.0.join(format!("{}.json.lock", "a".repeat(64))))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(cache.flush().is_err());
    assert_eq!(cache.pending.len(), 1);
    drop(lock);
    cache.flush().unwrap();
    assert!(cache.pending.is_empty());
    assert_eq!(directory.store().read().unwrap().len(), 1);
}
#[test]
fn malformed_or_wrong_identity_files_are_preserved_and_rejected() {
    let directory = Directory::new();
    let path = directory.0.join(format!("{}.json.zlib", "a".repeat(64)));
    fs::write(&path, b"broken compression").unwrap();
    assert!(Cache::with_store(directory.store()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken compression");
    fs::remove_file(&path).unwrap();
    directory
        .store()
        .merge(vec![disk::Entry {
            source: "入力".into(),
            target: "출력".into(),
        }])
        .unwrap();
    let other = directory.0.join(format!("{}.json.zlib", "b".repeat(64)));
    fs::copy(&path, &other).unwrap();
    assert!(disk::Store::new(&directory.0, "b".repeat(64))
        .unwrap()
        .read()
        .is_err());
}
#[test]
fn an_interrupted_replacement_reads_the_backup_without_overwriting_it() {
    let directory = Directory::new();
    directory
        .store()
        .merge(vec![disk::Entry {
            source: "入力".into(),
            target: "출력".into(),
        }])
        .unwrap();
    let path = directory.0.join(format!("{}.json.zlib", "a".repeat(64)));
    let backup = path.with_extension("backup");
    fs::rename(&path, &backup).unwrap();
    let before = fs::read(&backup).unwrap();
    let mut cache = Cache::with_store(directory.store()).unwrap();
    assert_eq!(cache.get("入力").as_deref(), Some("출력"));
    cache.insert("次", "다음".into());
    assert!(cache.flush().is_err());
    assert_eq!(fs::read(&backup).unwrap(), before);
}
#[test]
fn snapshots_enforce_bounded_entries_and_freshest_updates() {
    let directory = Directory::new();
    let entries = (0..MAX_ENTRIES + 2)
        .map(|i| disk::Entry {
            source: format!("原文{i}"),
            target: format!("번역{i}"),
        })
        .collect();
    directory.store().merge(entries).unwrap();
    let restored = directory.store().read().unwrap();
    assert_eq!(restored.len(), MAX_ENTRIES);
    assert_eq!(restored[0].source, "原文2");
    assert!(directory
        .store()
        .merge(vec![disk::Entry {
            source: "x".repeat(4097),
            target: "번역".into()
        }])
        .is_err());
}
#[test]
fn executable_memory_and_inference_changes_invalidate_the_namespace() {
    use crate::learning::Memory;
    use crate::offline::ParallelPolicy;
    let memory = Memory::default();
    let key = identity::fingerprint("program-a", &memory, ParallelPolicy::Parallel, 32, 4).unwrap();
    assert_eq!(
        key,
        identity::fingerprint("program-a", &memory, ParallelPolicy::Parallel, 32, 4).unwrap()
    );
    let mut changed = memory.clone();
    changed.learn("原文", "사용자 교정").unwrap();
    for other in [
        identity::fingerprint("program-b", &memory, ParallelPolicy::Parallel, 32, 4),
        identity::fingerprint("program-a", &changed, ParallelPolicy::Parallel, 32, 4),
        identity::fingerprint("program-a", &memory, ParallelPolicy::Serial, 32, 4),
        identity::fingerprint("program-a", &memory, ParallelPolicy::Parallel, 128, 4),
        identity::fingerprint("program-a", &memory, ParallelPolicy::Parallel, 32, 1),
    ] {
        assert_ne!(key, other.unwrap());
    }
}
