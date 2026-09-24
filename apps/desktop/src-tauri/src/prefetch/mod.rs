//! Adaptive sequential prefetch window (sealed tunables).

use crate::stream_seal::{prefetch_base_window, prefetch_grow_after, prefetch_max_window};
use parking_lot::Mutex;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct PrefetchEngine {
    state: Mutex<HashMap<String, FileState>>,
}

#[derive(Debug, Default, Clone)]
struct FileState {
    last_block: Option<u64>,
    sequential_hits: u32,
    window: u32,
}

impl PrefetchEngine {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Observe a user read of `block_index`; returns how many blocks ahead to fetch.
    pub fn observe(&self, file_id: &str, block_index: u64) -> u32 {
        let base = prefetch_base_window();
        let grow_after = prefetch_grow_after();
        let max_w = prefetch_max_window();
        let mut g = self.state.lock();
        let entry = g.entry(file_id.to_string()).or_insert_with(|| FileState {
            last_block: None,
            sequential_hits: 0,
            window: base,
        });
        if let Some(prev) = entry.last_block {
            if block_index == prev + 1 || block_index == prev {
                if block_index == prev + 1 {
                    entry.sequential_hits = entry.sequential_hits.saturating_add(1);
                    if entry.sequential_hits >= grow_after {
                        entry.window = (entry.window + 1).min(max_w);
                    }
                }
            } else {
                entry.sequential_hits = 0;
                entry.window = base;
            }
        }
        entry.last_block = Some(block_index);
        entry.window
    }

    pub fn reset_file(&self, file_id: &str) {
        self.state.lock().remove(file_id);
    }
}
