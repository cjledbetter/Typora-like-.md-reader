//! Kernel filesystem watcher with parent directory monitoring, debouncing, and self-save suppression.
//!
//! Provides real-time notifications for file modifications and deletions without losing
//! watches during atomic file replacement (which replace the target inode).

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Events emitted by [`DocumentWatcher`] to the application event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    /// Target file modified externally on disk (or created/replaced).
    Modified(PathBuf),
    /// Target file removed or moved out of the watched directory.
    Deleted(PathBuf),
}

/// Resolves the parent directory for a path, defaulting to `.` if parent is empty.
fn resolve_parent(path: &Path) -> PathBuf {
    if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            parent.to_path_buf()
        }
    } else {
        PathBuf::from(".")
    }
}

/// Kernel-level file watcher using `notify` with inotify parent-directory monitoring,
/// 50ms debouncing window, and timestamp-based self-save suppression.
pub struct DocumentWatcher {
    watcher: RecommendedWatcher,
    active_target: Arc<Mutex<Option<PathBuf>>>,
    current_parent: Option<PathBuf>,
    last_self_saved_mtime: Arc<AtomicU64>,
    target_generation: Arc<AtomicU64>,
    stop_signal: Arc<AtomicBool>,
    worker_handle: Option<thread::JoinHandle<()>>,
    event_tx: Sender<notify::Result<Event>>,
}

impl DocumentWatcher {
    /// Creates a new `DocumentWatcher` that invokes `callback` on debounced external events.
    pub fn new<F: Fn(WatcherEvent) + Send + 'static>(callback: F) -> Result<Self, notify::Error> {
        let (tx, rx): (Sender<notify::Result<Event>>, Receiver<notify::Result<Event>>) = channel();
        let notify_tx = tx.clone();

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let _ = notify_tx.send(res);
        })?;

        let active_target: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let last_self_saved_mtime = Arc::new(AtomicU64::new(0));
        let target_generation = Arc::new(AtomicU64::new(0));
        let stop_signal = Arc::new(AtomicBool::new(false));

        let active_clone = Arc::clone(&active_target);
        let self_mtime_clone = Arc::clone(&last_self_saved_mtime);
        let gen_clone = Arc::clone(&target_generation);
        let stop_clone = Arc::clone(&stop_signal);

        let worker_handle = thread::spawn(move || {
            let mut pending_event: Option<WatcherEvent> = None;
            let mut debounce_deadline: Option<Instant> = None;
            let mut last_seen_gen: u64 = 0;

            loop {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }

                // Discard pending events if active target switched
                let current_gen = gen_clone.load(Ordering::Relaxed);
                if current_gen != last_seen_gen {
                    last_seen_gen = current_gen;
                    pending_event = None;
                    debounce_deadline = None;
                }

                let timeout = match debounce_deadline {
                    Some(deadline) => {
                        let now = Instant::now();
                        if now >= deadline {
                            Duration::from_millis(0)
                        } else {
                            deadline - now
                        }
                    }
                    None => Duration::from_millis(100),
                };

                let res = rx.recv_timeout(timeout);

                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }

                match res {
                    Ok(Ok(event)) => {
                        // Ignore non-mutating access events (e.g., IN_OPEN, IN_ACCESS, IN_CLOSE_NOWRITE
                        // emitted when reading the file), preventing read -> notify -> read loops.
                        let is_mutating_event = matches!(
                            event.kind,
                            EventKind::Create(_)
                                | EventKind::Modify(_)
                                | EventKind::Remove(_)
                                | EventKind::Access(AccessKind::Close(AccessMode::Write))
                                | EventKind::Any
                        );
                        if !is_mutating_event {
                            continue;
                        }

                        let target_opt = {
                            let guard = active_clone.lock().unwrap();
                            guard.clone()
                        };

                        if let Some(target) = target_opt {
                            let canon_opt = fs::canonicalize(&target).ok();
                            let target_name = target.file_name();
                            let canon_name = canon_opt.as_ref().and_then(|c| c.file_name());

                            let is_match = event.paths.iter().any(|p| {
                                if p == &target {
                                    return true;
                                }
                                if let Some(ref canon) = canon_opt {
                                    if p == canon {
                                        return true;
                                    }
                                }
                                if let Some(name) = target_name {
                                    if p.file_name() == Some(name) {
                                        return true;
                                    }
                                }
                                if let Some(name) = canon_name {
                                    if p.file_name() == Some(name) {
                                        return true;
                                    }
                                }
                                false
                            });

                            if is_match {
                                let is_deleted = !target.exists() || event.kind.is_remove();
                                let ev = if is_deleted {
                                    WatcherEvent::Deleted(target.clone())
                                } else {
                                    WatcherEvent::Modified(target.clone())
                                };

                                pending_event = Some(ev);
                                debounce_deadline = Some(Instant::now() + Duration::from_millis(50));
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        // Inotify error, ignore
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if let Some(deadline) = debounce_deadline {
                            if Instant::now() >= deadline {
                                if let Some(ev) = pending_event.take() {
                                    let should_suppress = match &ev {
                                        WatcherEvent::Modified(path) => {
                                            let self_mtime = self_mtime_clone.load(Ordering::SeqCst);
                                            if self_mtime != 0 {
                                                if let Ok(meta) = fs::metadata(path) {
                                                    if let Ok(mod_time) = meta.modified() {
                                                        let cur_mtime = mod_time
                                                            .duration_since(UNIX_EPOCH)
                                                            .unwrap_or_default()
                                                            .as_secs();
                                                        cur_mtime == self_mtime
                                                    } else {
                                                        false
                                                    }
                                                } else {
                                                    false
                                                }
                                            } else {
                                                false
                                            }
                                        }
                                        WatcherEvent::Deleted(_) => false,
                                    };

                                    if !should_suppress {
                                        callback(ev);
                                    }
                                }
                                debounce_deadline = None;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            watcher,
            active_target,
            current_parent: None,
            last_self_saved_mtime,
            target_generation,
            stop_signal,
            worker_handle: Some(worker_handle),
            event_tx: tx,
        })
    }

    /// Dynamically targets a new active Markdown file to monitor.
    ///
    /// Un-watches the previous parent directory if different and attaches an inotify
    /// watch to the new parent directory.
    pub fn watch_file(&mut self, path: &Path) -> Result<(), notify::Error> {
        let parent = resolve_parent(path);

        if let Some(ref current) = self.current_parent {
            if current != &parent {
                let _ = self.watcher.unwatch(current);
                self.watcher.watch(&parent, RecursiveMode::NonRecursive)?;
                self.current_parent = Some(parent.clone());
            }
        } else {
            self.watcher.watch(&parent, RecursiveMode::NonRecursive)?;
            self.current_parent = Some(parent.clone());
        }

        let mut guard = self.active_target.lock().unwrap();
        *guard = Some(path.to_path_buf());
        self.target_generation.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    /// Sets the last self-saved modification timestamp in seconds since UNIX epoch.
    ///
    /// When the kernel inotify event arrives with a matching mtime, it is suppressed
    /// from triggering an application reload. Set to `0` to disable suppression.
    pub fn mark_self_saved(&mut self, mtime: u64) {
        self.last_self_saved_mtime.store(mtime, Ordering::SeqCst);
    }
}

impl Drop for DocumentWatcher {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        let _ = self.event_tx.send(Ok(Event::new(EventKind::Other)));
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        if let Some(parent) = self.current_parent.take() {
            let _ = self.watcher.unwatch(&parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new() -> Self {
            let pid = std::process::id();
            let timestamp = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir_name = format!("watcher_unit_{}_{}", pid, timestamp);
            let root = std::env::temp_dir().join(dir_name);
            fs::create_dir_all(&root).expect("Failed to create test sandbox");
            Self { root }
        }

        fn create_file(&self, name: &str, content: &str) -> PathBuf {
            let path = self.root.join(name);
            let mut file = fs::File::create(&path).expect("Failed to create file");
            file.write_all(content.as_bytes()).expect("Failed to write content");
            file.sync_all().expect("Failed to sync file");
            path
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn test_unit_detects_external_file_modification() {
        let sandbox = TestSandbox::new();
        let file = sandbox.create_file("unit_test.md", "Initial");

        let (tx, rx) = channel();
        let mut watcher = DocumentWatcher::new(move |ev| {
            let _ = tx.send(ev);
        }).expect("DocumentWatcher::new failed");

        watcher.watch_file(&file).expect("watch_file failed");
        thread::sleep(Duration::from_millis(50));
        thread::sleep(Duration::from_millis(1100));

        fs::write(&file, "Modified externally").unwrap();

        let ev = rx.recv_timeout(Duration::from_millis(600)).expect("Event timeout");
        assert_eq!(ev, WatcherEvent::Modified(file));
    }

    #[test]
    fn test_unit_self_save_suppressed_when_mtime_matches() {
        let sandbox = TestSandbox::new();
        let file = sandbox.create_file("unit_self.md", "Initial");

        let (tx, rx) = channel();
        let mut watcher = DocumentWatcher::new(move |ev| {
            let _ = tx.send(ev);
        }).expect("DocumentWatcher::new failed");

        watcher.watch_file(&file).expect("watch_file failed");
        thread::sleep(Duration::from_millis(50));
        thread::sleep(Duration::from_millis(1100));

        // Atomic write simulation
        let tmp = sandbox.root.join(".unit_tmp");
        fs::write(&tmp, "New internal content").unwrap();
        fs::rename(&tmp, &file).unwrap();

        let mtime = fs::metadata(&file).unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs();
        watcher.mark_self_saved(mtime);

        let res = rx.recv_timeout(Duration::from_millis(300));
        assert!(res.is_err(), "Self-saved event must be suppressed");
    }

    #[test]
    fn test_unit_debouncing_collapses_burst_writes() {
        let sandbox = TestSandbox::new();
        let file = sandbox.create_file("unit_burst.md", "0");

        let count = Arc::new(AtomicU64::new(0));
        let count_clone = Arc::clone(&count);
        let mut watcher = DocumentWatcher::new(move |_| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }).expect("DocumentWatcher::new failed");

        watcher.watch_file(&file).expect("watch_file failed");
        thread::sleep(Duration::from_millis(50));
        thread::sleep(Duration::from_millis(1100));

        for i in 1..=5 {
            fs::write(&file, format!("{}", i)).unwrap();
        }

        thread::sleep(Duration::from_millis(300));
        let total = count.load(Ordering::SeqCst);
        assert!(total <= 2, "Debouncer must collapse rapid writes, got: {}", total);
    }

    #[test]
    fn test_unit_reading_watched_file_does_not_trigger_modified() {
        let sandbox = TestSandbox::new();
        let file = sandbox.create_file("unit_read_only.md", "# Read Only Test");

        let (tx, rx) = channel();
        let mut watcher = DocumentWatcher::new(move |ev| {
            let _ = tx.send(ev);
        }).expect("DocumentWatcher::new failed");

        watcher.watch_file(&file).expect("watch_file failed");
        thread::sleep(Duration::from_millis(100));

        // Drain any initial creation events
        while rx.try_recv().is_ok() {}

        // Read the file multiple times (simulating app loading / reading the file)
        for _ in 0..5 {
            let content = fs::read_to_string(&file).expect("Failed to read file");
            assert_eq!(content, "# Read Only Test");
            thread::sleep(Duration::from_millis(20));
        }

        let res = rx.recv_timeout(Duration::from_millis(250));
        assert!(
            res.is_err(),
            "Reading a watched file must not trigger a spurious Modified event, got: {:?}",
            res
        );
    }
}
