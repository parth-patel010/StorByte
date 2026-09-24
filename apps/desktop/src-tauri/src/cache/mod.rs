//! Encrypted RAM block cache for the fast-fetch path.
//!
//! Payloads are ChaCha20-sealed with a process session key so memory dumps and
//! casual inspection do not reveal plaintext file bytes.

use crate::stream_seal::{block_size, min_working_set, seal_copy, unseal_copy};
use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

struct Entry {
    /// Ciphertext (same length as plaintext).
    data: Bytes,
    plain_len: usize,
    /// When false, stored bytes are a prefix of the block starting at offset 0.
    complete: bool,
    last_accessed: u64,
}

pub struct BlockCache {
    inner: Mutex<Inner>,
}

struct Inner {
    max_bytes: u64,
    used: u64,
    map: HashMap<(String, u64), Entry>,
}

impl BlockCache {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            inner: Mutex::new(Inner {
                max_bytes,
                used: 0,
                map: HashMap::new(),
            }),
        }
    }

    pub fn set_max_bytes(&self, max_bytes: u64) {
        let mut g = self.inner.lock();
        g.max_bytes = max_bytes;
        Self::evict_locked(&mut g);
    }

    fn capacity(g: &Inner) -> u64 {
        g.max_bytes.max(min_working_set())
    }

    pub fn contains_complete(&self, file_id: &str, block_index: u64) -> bool {
        let g = self.inner.lock();
        g.map
            .get(&(file_id.to_string(), block_index))
            .map(|e| e.complete)
            .unwrap_or(false)
    }

    pub fn get_range(
        &self,
        file_id: &str,
        block_index: u64,
        offset_in_block: u64,
        length: u64,
    ) -> Option<Bytes> {
        let mut g = self.inner.lock();
        let key = (file_id.to_string(), block_index);
        let e = g.map.get_mut(&key)?;
        let start = offset_in_block as usize;
        let end = start.saturating_add(length as usize);
        if end > e.plain_len {
            return None;
        }
        e.last_accessed = now_secs();
        let plain = unseal_copy(file_id, block_index, &e.data);
        Some(Bytes::from(plain).slice(start..end))
    }

    pub fn get(&self, file_id: &str, block_index: u64) -> Option<Bytes> {
        let mut g = self.inner.lock();
        let key = (file_id.to_string(), block_index);
        if let Some(e) = g.map.get_mut(&key) {
            e.last_accessed = now_secs();
            let plain = unseal_copy(file_id, block_index, &e.data);
            return Some(Bytes::from(plain));
        }
        None
    }

    pub fn put(&self, file_id: &str, block_index: u64, data: impl Into<Bytes>) {
        self.put_entry(file_id, block_index, data.into(), true);
    }

    pub fn put_prefix(&self, file_id: &str, block_index: u64, data: impl Into<Bytes>) {
        let data = data.into();
        if data.is_empty() {
            return;
        }
        let mut g = self.inner.lock();
        let key = (file_id.to_string(), block_index);
        if let Some(old) = g.map.get(&key) {
            if old.complete || old.plain_len >= data.len() {
                return;
            }
        }
        if let Some(old) = g.map.remove(&key) {
            g.used = g.used.saturating_sub(old.data.len() as u64);
        }
        let sealed = Bytes::from(seal_copy(file_id, block_index, &data));
        let len = sealed.len() as u64;
        let plain_len = data.len();
        g.map.insert(
            key,
            Entry {
                data: sealed,
                plain_len,
                complete: false,
                last_accessed: now_secs(),
            },
        );
        g.used += len;
        Self::evict_locked(&mut g);
    }

    fn put_entry(&self, file_id: &str, block_index: u64, data: Bytes, complete: bool) {
        let mut g = self.inner.lock();
        let key = (file_id.to_string(), block_index);
        let sealed = Bytes::from(seal_copy(file_id, block_index, &data));
        let len = sealed.len() as u64;
        let plain_len = data.len();
        if let Some(old) = g.map.remove(&key) {
            g.used = g.used.saturating_sub(old.data.len() as u64);
        }
        g.map.insert(
            key,
            Entry {
                data: sealed,
                plain_len,
                complete,
                last_accessed: now_secs(),
            },
        );
        g.used += len;
        Self::evict_locked(&mut g);
    }

    pub fn invalidate_file(&self, file_id: &str) {
        let mut g = self.inner.lock();
        let keys: Vec<_> = g
            .map
            .keys()
            .filter(|(fid, _)| fid == file_id)
            .cloned()
            .collect();
        for key in keys {
            if let Some(e) = g.map.remove(&key) {
                g.used = g.used.saturating_sub(e.data.len() as u64);
            }
        }
    }

    pub fn clear(&self) {
        let mut g = self.inner.lock();
        g.map.clear();
        g.used = 0;
    }

    fn evict_locked(g: &mut Inner) {
        let cap = Self::capacity(g);
        while g.used > cap && !g.map.is_empty() {
            let victim = g
                .map
                .iter()
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(k, _)| k.clone());
            if let Some(k) = victim {
                if let Some(e) = g.map.remove(&k) {
                    g.used = g.used.saturating_sub(e.data.len() as u64);
                }
            } else {
                break;
            }
        }
    }

    pub fn stats(&self) -> (u64, u64, usize) {
        let g = self.inner.lock();
        (g.used, Self::capacity(&g), g.map.len())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn approx_block_capacity(max_bytes: u64) -> u64 {
    let cap = max_bytes.max(min_working_set());
    let bs = block_size();
    if bs == 0 {
        0
    } else {
        cap / bs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidate_file_drops_only_that_file() {
        let cache = BlockCache::new(64 * 1024 * 1024);
        cache.put("a", 0, vec![1; 100]);
        cache.put("a", 1, vec![2; 100]);
        cache.put("b", 0, vec![3; 100]);
        cache.invalidate_file("a");
        assert!(cache.get("a", 0).is_none());
        assert!(cache.get("a", 1).is_none());
        assert!(cache.get("b", 0).is_some());
        let (used, _, n) = cache.stats();
        assert_eq!(n, 1);
        assert_eq!(used, 100);
    }

    #[test]
    fn prefix_serves_head_then_full_replaces() {
        let cache = BlockCache::new(64 * 1024 * 1024);
        cache.put_prefix("f", 0, vec![9u8; 64]);
        assert!(cache.get_range("f", 0, 0, 32).is_some());
        assert!(cache.get_range("f", 0, 0, 128).is_none());
        assert!(!cache.contains_complete("f", 0));
        cache.put("f", 0, vec![1u8; 256]);
        assert!(cache.contains_complete("f", 0));
        assert_eq!(cache.get_range("f", 0, 0, 256).unwrap().len(), 256);
    }

    #[test]
    fn cached_bytes_are_not_plaintext_in_map() {
        let cache = BlockCache::new(64 * 1024 * 1024);
        let plain = vec![0x41u8; 64];
        cache.put("x", 0, plain.clone());
        let g = cache.inner.lock();
        let e = g.map.get(&("x".into(), 0)).unwrap();
        assert_ne!(&e.data[..], &plain[..]);
    }
}
