//! One bounded cache per translator; workers share results without duplicating
//! sentence strings in four independent caches.
mod disk;
mod identity;
#[cfg(test)]
mod persistent_tests;
use anyhow::Result;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const MAX_ENTRIES: usize = 16_384;
pub(super) const MAX_BYTES: usize = 16 * 1024 * 1024;
pub(super) fn valid(source: &str, target: &str) -> bool {
    !source.trim().is_empty()
        && !target.trim().is_empty()
        && source.len() <= 4096
        && target.len() <= 8192
        && !source.contains('\0')
        && !target.contains('\0')
        && !target.contains(super::text::REVIEW_MARKER)
}
pub(super) fn cost(source: &str, target: &str) -> usize {
    source.len() + target.len() + 128
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CacheStats {
    pub restored_entries: usize,
    pub hit_lookups: u64,
    pub miss_lookups: u64,
    pub saved_entries: usize,
}
pub(super) struct Cache {
    values: HashMap<String, String>,
    order: VecDeque<String>,
    limit: usize,
    bytes: usize,
    disk: Option<disk::Store>,
    pending: HashSet<String>,
    hits: AtomicU64,
    misses: AtomicU64,
    restored: usize,
    saved: usize,
    warned: bool,
}
impl Default for Cache {
    fn default() -> Self {
        Self::new(1024)
    }
}
impl Cache {
    fn new(limit: usize) -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
            limit: limit.max(1),
            bytes: 0,
            disk: None,
            pending: HashSet::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            restored: 0,
            saved: 0,
            warned: false,
        }
    }
    pub(super) fn persistent(
        directory: &Path,
        memory: &crate::learning::Memory,
        policy: super::ParallelPolicy,
        workers: usize,
        cpus: usize,
    ) -> Result<Self> {
        let key = identity::current(memory, policy, workers, cpus)?;
        Self::with_store(disk::Store::new(directory, key)?)
    }
    fn with_store(store: disk::Store) -> Result<Self> {
        let restored = store.read()?;
        let mut cache = Self::new(MAX_ENTRIES);
        for entry in restored {
            cache.insert(&entry.source, entry.target);
        }
        cache.restored = cache.values.len();
        cache.disk = Some(store);
        Ok(cache)
    }
    pub(super) fn contains(&self, source: &str) -> bool {
        self.values.contains_key(source)
    }
    pub(super) fn get(&self, source: &str) -> Option<String> {
        let value = self.values.get(source);
        if value.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        value.cloned()
    }
    pub(super) fn insert(&mut self, source: &str, text: String) {
        if !valid(source, &text) {
            return;
        }
        let new_cost = cost(source, &text);
        if let Some(value) = self.values.get_mut(source) {
            self.bytes -= cost(source, value);
            *value = text;
        } else {
            self.values.insert(source.to_owned(), text);
            self.order.push_back(source.to_owned());
        }
        self.bytes += new_cost;
        if self.disk.is_some() {
            self.pending.insert(source.to_owned());
        }
        while self.values.len() > self.limit || self.bytes > MAX_BYTES {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(value) = self.values.remove(&old) {
                self.bytes -= cost(&old, &value);
            }
            self.pending.remove(&old);
        }
        if self.pending.len() >= 512 {
            self.flush_best_effort();
        }
    }
    pub(super) fn evict(&mut self, sources: &[String]) {
        for source in sources {
            if let Some(value) = self.values.remove(source) {
                self.bytes -= cost(source, &value);
            }
            self.pending.remove(source);
        }
        self.order.retain(|key| !sources.contains(key));
    }
    pub(super) fn stats(&self) -> CacheStats {
        CacheStats {
            restored_entries: self.restored,
            hit_lookups: self.hits.load(Ordering::Relaxed),
            miss_lookups: self.misses.load(Ordering::Relaxed),
            saved_entries: self.saved,
        }
    }
    pub(super) fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let Some(store) = &self.disk else {
            return Ok(());
        };
        let entries = self
            .order
            .iter()
            .filter(|key| self.pending.contains(*key))
            .filter_map(|key| {
                self.values.get(key).map(|value| disk::Entry {
                    source: key.clone(),
                    target: value.clone(),
                })
            })
            .collect::<Vec<_>>();
        store.merge(entries)?;
        self.saved += self.pending.len();
        self.pending.clear();
        Ok(())
    }
    fn flush_best_effort(&mut self) {
        if let Err(error) = self.flush() {
            if !self.warned {
                eprintln!("번역 결과 재사용 저장을 건너뜁니다: {error:#}");
                self.warned = true;
            }
        }
    }
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.flush_best_effort();
        if self.disk.is_some() {
            let stats = self.stats();
            eprintln!(
                "번역 결과 재사용 · 조회 적중 {}회 / 미적중 {}회 · 이전 결과 {}개",
                stats.hit_lookups, stats.miss_lookups, stats.restored_entries
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_do_not_duplicate_eviction_slots_and_size_is_bounded() {
        let mut c = Cache::new(2);
        c.insert("a", "1".into());
        c.insert("a", "2".into());
        c.insert("b", "3".into());
        c.insert("c", "4".into());
        assert_eq!(c.get("a"), None);
        assert_eq!(c.get("b"), Some("3".into()));
        assert_eq!(c.values.len(), 2);
        c.evict(&["b".into()]);
        c.insert("d", "5".into());
        assert_eq!(c.get("c"), Some("4".into()));
        c.insert(&"x".repeat(4097), "too big".into());
        assert_eq!(c.values.len(), 2);
    }
}
