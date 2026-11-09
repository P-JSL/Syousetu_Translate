//! One bounded cache per translator; workers share results without duplicating
//! sentence strings in four independent caches.
use std::collections::{HashMap, VecDeque};
pub(super) struct Cache {
    values: HashMap<String, String>,
    order: VecDeque<String>,
    limit: usize,
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
        }
    }
    pub(super) fn get(&self, source: &str) -> Option<String> {
        self.values.get(source).cloned()
    }
    pub(super) fn insert(&mut self, source: &str, text: String) {
        if source.len() > 4096 || text.len() > 8192 {
            return;
        }
        if let Some(value) = self.values.get_mut(source) {
            *value = text;
            return;
        }
        if self.values.len() >= self.limit {
            if let Some(old) = self.order.pop_front() {
                self.values.remove(&old);
            }
        }
        self.values.insert(source.to_owned(), text);
        self.order.push_back(source.to_owned());
    }
    pub(super) fn evict(&mut self, sources: &[String]) {
        for source in sources {
            self.values.remove(source);
        }
        self.order.retain(|key| !sources.contains(key));
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
