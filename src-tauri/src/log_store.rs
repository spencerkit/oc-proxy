//! Module Overview
//! In-memory bounded log storage used by the proxy runtime.
//! Implements append/list/clear with synchronized access and fixed retention size.

use crate::models::LogEntry;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LogStore {
    inner: Arc<Mutex<VecDeque<LogEntry>>>,
    limit: usize,
}

impl LogStore {
    pub fn new(limit: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(limit))),
            limit,
        }
    }

    /// Append one log entry and evict oldest entries when capacity is exceeded.
    pub fn append(&self, entry: LogEntry) {
        let mut guard = self.inner.lock().expect("log mutex poisoned");
        guard.push_back(entry);
        while guard.len() > self.limit {
            let _ = guard.pop_front();
        }
    }

    /// Return at most `max` latest logs in chronological order.
    pub fn list(&self, max: usize) -> Vec<LogEntry> {
        let guard = self.inner.lock().expect("log mutex poisoned");
        guard
            .iter()
            .rev()
            .take(max)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Remove all in-memory logs.
    pub fn clear(&self) {
        let mut guard = self.inner.lock().expect("log mutex poisoned");
        guard.clear();
    }
}
