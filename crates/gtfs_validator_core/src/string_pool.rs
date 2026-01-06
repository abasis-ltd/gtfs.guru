use compact_str::CompactString;
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use gtfs_guru_model::StringId;
use std::sync::atomic::{AtomicU32, Ordering};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct StringPool {
    inner: Arc<StringPoolInner>,
}

#[derive(Debug)]
struct StringPoolInner {
    map: DashMap<CompactString, StringId>,
    resolver: DashMap<u32, CompactString>,
    next_id: AtomicU32,
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

impl StringPool {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StringPoolInner {
                map: DashMap::new(),
                resolver: DashMap::new(),
                next_id: AtomicU32::new(1),
            }),
        }
    }

    pub fn intern(&self, s: &str) -> StringId {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return StringId(0);
        }
        let s_compact = CompactString::new(trimmed);
        match self.inner.map.entry(s_compact.clone()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let id_val = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
                let id = StringId(id_val);
                entry.insert(id);
                self.inner.resolver.insert(id_val, s_compact);
                id
            }
        }
    }

    pub fn resolve(&self, id: StringId) -> String {
        if id.0 == 0 {
            return String::new();
        }
        self.inner
            .resolver
            .get(&id.0)
            .map(|s| s.as_str().to_string())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    #[test]
    fn test_string_pool_basic() {
        let pool = StringPool::new();
        let id1 = pool.intern("test");
        let id2 = pool.intern("test");
        let id3 = pool.intern("other");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(pool.resolve(id1), "test");
        assert_eq!(pool.resolve(id3), "other");
        assert_eq!(pool.resolve(StringId(0)), "");
    }

    #[test]
    fn test_string_pool_parallel() {
        let pool = StringPool::new();
        (0..1000).into_par_iter().for_each(|i| {
            let s = format!("string_{}", i % 10);
            let id = pool.intern(&s);
            assert_eq!(pool.resolve(id), s);
        });
    }
}
