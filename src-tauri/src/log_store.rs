//! Module Overview
//! In-memory bounded log storage used by the proxy runtime.
//! Implements append/list/clear with synchronized access and fixed retention size.

use crate::models::LogEntry;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LogStore {
    inner: Arc<Mutex<VecDeque<LogEntry>>>,
    limit: usize,
    #[cfg(debug_assertions)]
    dev_log_sink: Option<Arc<Mutex<DevLogSink>>>,
}

#[cfg(debug_assertions)]
struct DevLogSink {
    path: PathBuf,
    file: std::fs::File,
}

impl LogStore {
    #[allow(dead_code)]
    pub fn new(limit: usize) -> Self {
        Self::with_dev_log_file(limit, None)
    }

    /// Create an in-memory log store with optional dev-only JSONL persistence.
    ///
    /// When compiled in debug mode and `dev_log_path` is provided, every appended
    /// log entry is also serialized as one JSON line into that file.
    pub fn with_dev_log_file(limit: usize, dev_log_path: Option<PathBuf>) -> Self {
        #[cfg(debug_assertions)]
        let dev_log_sink = init_dev_log_sink(dev_log_path);
        #[cfg(not(debug_assertions))]
        let _ = dev_log_path;
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(limit))),
            limit,
            #[cfg(debug_assertions)]
            dev_log_sink,
        }
    }

    /// Append one log entry and evict oldest entries when capacity is exceeded.
    pub fn append(&self, entry: LogEntry) {
        #[cfg(debug_assertions)]
        let entry_for_dev_file = entry.clone();
        let mut guard = self.inner.lock().expect("log mutex poisoned");
        guard.push_back(entry);
        while guard.len() > self.limit {
            let _ = guard.pop_front();
        }
        drop(guard);
        #[cfg(debug_assertions)]
        self.append_to_dev_file(&entry_for_dev_file);
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
        drop(guard);
        #[cfg(debug_assertions)]
        self.clear_dev_file();
    }

    #[cfg(debug_assertions)]
    fn append_to_dev_file(&self, entry: &LogEntry) {
        let Some(sink) = &self.dev_log_sink else {
            return;
        };
        let line = match serde_json::to_string(entry) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("dev log persist skipped: serialize failed: {err}");
                return;
            }
        };
        let mut guard = match sink.lock() {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Err(err) = std::io::Write::write_all(&mut guard.file, line.as_bytes()) {
            eprintln!("dev log persist failed: write failed: {err}");
            return;
        }
        if let Err(err) = std::io::Write::write_all(&mut guard.file, b"\n") {
            eprintln!("dev log persist failed: newline write failed: {err}");
            return;
        }
        let _ = std::io::Write::flush(&mut guard.file);
    }

    #[cfg(debug_assertions)]
    fn clear_dev_file(&self) {
        let Some(sink) = &self.dev_log_sink else {
            return;
        };
        let mut guard = match sink.lock() {
            Ok(v) => v,
            Err(_) => return,
        };
        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&guard.path)
        {
            Ok(file) => guard.file = file,
            Err(err) => eprintln!("dev log clear failed: {err}"),
        }
    }
}

#[cfg(debug_assertions)]
fn init_dev_log_sink(dev_log_path: Option<PathBuf>) -> Option<Arc<Mutex<DevLogSink>>> {
    let path = dev_log_path?;
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!("dev log disabled: create parent dir failed: {err}");
            return None;
        }
    }
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(v) => v,
        Err(err) => {
            eprintln!("dev log disabled: open file failed: {err}");
            return None;
        }
    };
    eprintln!("dev log persistence enabled: {}", path.display());
    Some(Arc::new(Mutex::new(DevLogSink { path, file })))
}
