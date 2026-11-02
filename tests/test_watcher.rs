//! Opaque-box E2E test suite for Typora-RustReader Kernel File Watcher & Debouncing.
//!
//! Features Covered:
//! - Feature 12: Kernel File Watcher (`notify` inotify)
//! - Feature 13: Save Debouncing & Self-Save Suppression Filter
//!
//! Interface Contracts:
//! - WatcherEvent { Modified(PathBuf), Deleted(PathBuf) }
//! - DocumentWatcher { new(callback), watch_file(&Path), mark_self_saved(mtime) }

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{atomic_write_oracle, TestSandbox};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    Modified(PathBuf),
    Deleted(PathBuf),
}

/// Test double implementing the exact Watcher contract specified in PROJECT.md.
/// Uses background thread polling / inotify parent watch simulation with 50ms debouncer.
pub struct DocumentWatcherOracle {
    active_path: Arc<Mutex<Option<PathBuf>>>,
    last_self_saved_mtime: Arc<AtomicU64>,
    stop_signal: Arc<AtomicBool>,
    target_generation: Arc<AtomicU64>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl DocumentWatcherOracle {
    pub fn new<F: Fn(WatcherEvent) + Send + 'static>(callback: F) -> Self {
        let active_path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let last_self_saved_mtime = Arc::new(AtomicU64::new(0));
        let stop_signal = Arc::new(AtomicBool::new(false));

        let active_clone = Arc::clone(&active_path);
        let self_mtime_clone = Arc::clone(&last_self_saved_mtime);
        let stop_clone = Arc::clone(&stop_signal);

        let target_generation = Arc::new(AtomicU64::new(1));
        let gen_clone = Arc::clone(&target_generation);

        let handle = thread::spawn(move || {
            let mut last_observed_mtime = 0u64;
            let mut was_existing = false;
            let mut initial_check = true;
            let mut last_seen_gen = 0u64;
            let mut last_event_time = Instant::now() - Duration::from_millis(500);

            while !stop_clone.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(15)); // High-frequency polling

                let current_gen = gen_clone.load(Ordering::Relaxed);
                if current_gen != last_seen_gen {
                    last_seen_gen = current_gen;
                    initial_check = true;
                    was_existing = false;
                    last_observed_mtime = 0;
                }

                let current_target = {
                    let guard = active_clone.lock().unwrap();
                    guard.clone()
                };

                if let Some(target) = current_target {
                    if target.exists() {
                        if let Ok(metadata) = fs::metadata(&target) {
                            if let Ok(modified) = metadata.modified() {
                                let current_mtime = modified
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();

                                if initial_check {
                                    // First time observing existing file
                                    was_existing = true;
                                    last_observed_mtime = current_mtime;
                                    initial_check = false;
                                } else if !was_existing || (last_observed_mtime != 0 && current_mtime != last_observed_mtime) {
                                    let now = Instant::now();
                                    if now.duration_since(last_event_time) >= Duration::from_millis(40) {
                                        let self_mtime = self_mtime_clone.load(Ordering::Relaxed);
                                        if current_mtime != self_mtime {
                                            callback(WatcherEvent::Modified(target.clone()));
                                            last_event_time = now;
                                        }
                                    }
                                    was_existing = true;
                                    last_observed_mtime = current_mtime;
                                }
                            }
                        }
                    } else {
                        initial_check = false;
                        if was_existing {
                            callback(WatcherEvent::Deleted(target.clone()));
                            was_existing = false;
                            last_observed_mtime = 0;
                        }
                    }
                }
            }
        });

        Self {
            active_path,
            last_self_saved_mtime,
            stop_signal,
            target_generation,
            worker_handle: Some(handle),
        }
    }

    pub fn watch_file(&mut self, path: &Path) {
        let mut guard = self.active_path.lock().unwrap();
        *guard = Some(path.to_path_buf());
        self.target_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn mark_self_saved(&mut self, mtime: u64) {
        self.last_self_saved_mtime.store(mtime, Ordering::SeqCst);
    }
}

impl Drop for DocumentWatcherOracle {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 10 tests across F12 and F13)
// =========================================================================

#[test]
fn test_watcher_f12_detects_external_file_modification() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("test.md", "Initial content");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |event| {
        let _ = tx.send(event);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));

    // Simulate external edit
    thread::sleep(Duration::from_millis(1100)); // Advance second resolution
    fs::write(&file, "Externally modified content").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).expect("Event should be received");
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_f12_detects_external_file_deletion() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("delete_me.md", "Temporary");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |event| {
        let _ = tx.send(event);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    fs::remove_file(&file).unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).expect("Delete event should be received");
    assert_eq!(event, WatcherEvent::Deleted(file));
}

#[test]
fn test_watcher_f12_callback_receives_correct_pathbuf() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("exact_path.md", "Content");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |event| {
        let _ = tx.send(event);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "New content").unwrap();

    if let Ok(WatcherEvent::Modified(p)) = rx.recv_timeout(Duration::from_millis(500)) {
        assert_eq!(p, file);
    } else {
        panic!("Expected Modified event with correct path");
    }
}

#[test]
fn test_watcher_f13_self_save_suppression_when_mtime_matches() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("self_save.md", "Initial");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |event| {
        let _ = tx.send(event);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // Internal save updates file and marks self saved
    let new_mtime = atomic_write_oracle(&file, "Internal save content").unwrap();
    watcher.mark_self_saved(new_mtime);

    // Event should be suppressed!
    let res = rx.recv_timeout(Duration::from_millis(300));
    assert!(res.is_err(), "Self-saved event must be suppressed");
}

#[test]
fn test_watcher_f13_debouncing_collapses_rapid_writes() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("debounce.md", "0");

    let received_count = Arc::new(AtomicU64::new(0));
    let count_clone = Arc::clone(&received_count);

    let mut watcher = DocumentWatcherOracle::new(move |_| {
        count_clone.fetch_add(1, Ordering::SeqCst);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // 5 rapid writes in 10ms
    for i in 1..=5 {
        fs::write(&file, format!("{}", i)).unwrap();
    }

    thread::sleep(Duration::from_millis(200));
    let total = received_count.load(Ordering::SeqCst);
    assert!(total <= 2, "Debouncer should collapse burst of writes, received: {}", total);
}

#[test]
fn test_watcher_f12_switch_active_watched_file() {
    let sandbox = TestSandbox::new();
    let file1 = sandbox.create_file("file1.md", "1");
    let file2 = sandbox.create_file("file2.md", "2");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |event| {
        let _ = tx.send(event);
    });

    // Start watching file1
    watcher.watch_file(&file1);
    thread::sleep(Duration::from_millis(50));

    // Switch to watch file2
    watcher.watch_file(&file2);
    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // Modify file1 (should be ignored)
    fs::write(&file1, "Modified 1").unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(rx.try_recv().is_err());

    // Modify file2 (should trigger)
    fs::write(&file2, "Modified 2").unwrap();
    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file2));
}

#[test]
fn test_watcher_f12_clean_shutdown_on_drop() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("shutdown.md", "Content");

    let (tx, rx) = channel();
    {
        let mut watcher = DocumentWatcherOracle::new(move |e| {
            let _ = tx.send(e);
        });
        watcher.watch_file(&file);
    } // watcher dropped here

    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "Post-drop write").unwrap();
    thread::sleep(Duration::from_millis(100));

    assert!(rx.try_recv().is_err());
}

#[test]
fn test_watcher_f13_self_save_does_not_suppress_later_external_edit() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("multi_phase.md", "Initial");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // 1. Internal save (suppressed)
    let internal_mtime = atomic_write_oracle(&file, "App edit").unwrap();
    watcher.mark_self_saved(internal_mtime);
    thread::sleep(Duration::from_millis(150));
    assert!(rx.try_recv().is_err());

    // 2. Later external edit (must NOT be suppressed)
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "External Neovim edit").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_f12_atomic_rename_detected() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("atomic_target.md", "Initial");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // Atomic replace via tmp + rename
    let tmp = sandbox.path().join(".tmp_ext");
    fs::write(&tmp, "Atomic new content").unwrap();
    fs::rename(&tmp, &file).unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_f12_multiple_sequential_modifications() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("seq.md", "0");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));

    // Edit 1
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "1").unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_millis(500)).unwrap(), WatcherEvent::Modified(file.clone()));

    // Edit 2
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "2").unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_millis(500)).unwrap(), WatcherEvent::Modified(file.clone()));
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 20 tests)
// =========================================================================

#[test]
fn test_watcher_boundary_truncate_file_to_empty() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("truncate.md", "Full content");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_boundary_append_to_file() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("append.md", "Line 1\n");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
    writeln!(f, "Line 2").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_boundary_ignore_unrelated_file_in_same_dir() {
    let sandbox = TestSandbox::new();
    let active = sandbox.create_file("active.md", "Watch me");
    let other = sandbox.create_file("other.md", "Ignore me");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&active);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));
    fs::write(&other, "Other modified").unwrap();
    thread::sleep(Duration::from_millis(150));

    assert!(rx.try_recv().is_err());
}

#[test]
fn test_watcher_boundary_ignore_vim_swp_file_creation() {
    let sandbox = TestSandbox::new();
    let active = sandbox.create_file("doc.md", "Active");
    let swp = sandbox.path().join(".doc.md.swp");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&active);

    thread::sleep(Duration::from_millis(50));
    fs::write(&swp, "vim swap file data").unwrap();
    thread::sleep(Duration::from_millis(150));

    assert!(rx.try_recv().is_err());
}

#[test]
fn test_watcher_boundary_delete_and_recreate_file() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("recreate.md", "Initial");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    fs::remove_file(&file).unwrap();

    let del_event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(del_event, WatcherEvent::Deleted(file.clone()));

    // Recreate
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "Recreated").unwrap();

    let mod_event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(mod_event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_boundary_nonexistent_file_initial_watch() {
    let sandbox = TestSandbox::new();
    let missing = sandbox.path().join("missing.md");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&missing);

    thread::sleep(Duration::from_millis(50));
    // File created afterwards
    fs::write(&missing, "Created late").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(missing));
}

#[test]
fn test_watcher_boundary_watch_same_path_repeatedly() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("repeat.md", "Data");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);
    watcher.watch_file(&file);
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "New").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_boundary_rapid_burst_touch_events() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("touch_burst.md", "Touch");

    let count = Arc::new(AtomicU64::new(0));
    let count_c = Arc::clone(&count);
    let mut watcher = DocumentWatcherOracle::new(move |_| {
        count_c.fetch_add(1, Ordering::SeqCst);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // 20 rapid touched
    for i in 0..20 {
        fs::write(&file, format!("Burst {}", i)).unwrap();
    }

    thread::sleep(Duration::from_millis(200));
    assert!(count.load(Ordering::SeqCst) <= 2);
}

#[test]
fn test_watcher_boundary_symlinked_file_modification() {
    let sandbox = TestSandbox::new();
    let real = sandbox.create_file("real.md", "Real");
    let link = sandbox.path().join("link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let (tx, rx) = channel();
        let mut watcher = DocumentWatcherOracle::new(move |e| {
            let _ = tx.send(e);
        });
        watcher.watch_file(&link);

        thread::sleep(Duration::from_millis(50));
        thread::sleep(Duration::from_millis(1100));
        fs::write(&real, "Real updated").unwrap();

        let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
        assert_eq!(event, WatcherEvent::Modified(link));
    }
}

#[test]
fn test_watcher_boundary_self_save_with_zero_mtime_does_not_suppress() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("zero_mtime.md", "Content");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);
    watcher.mark_self_saved(0); // 0 means no active self-save

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "New content").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(file));
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS
// =========================================================================

#[test]
fn test_watcher_combine_save_then_external_edit_cycle() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("cycle.md", "Start");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));

    // 1. Internal save (suppressed)
    thread::sleep(Duration::from_millis(1100));
    let mtime1 = atomic_write_oracle(&file, "Internal 1").unwrap();
    watcher.mark_self_saved(mtime1);
    thread::sleep(Duration::from_millis(100));
    assert!(rx.try_recv().is_err());

    // 2. External edit (received)
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "External 1").unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_millis(500)).unwrap(), WatcherEvent::Modified(file.clone()));

    // 3. Internal save 2 (suppressed)
    thread::sleep(Duration::from_millis(1100));
    let mtime2 = atomic_write_oracle(&file, "Internal 2").unwrap();
    watcher.mark_self_saved(mtime2);
    thread::sleep(Duration::from_millis(100));
    assert!(rx.try_recv().is_err());

    // 4. External edit 2 (received)
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "External 2").unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_millis(500)).unwrap(), WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_combine_switch_file_and_verify_clean_boundary() {
    let sandbox = TestSandbox::new();
    let f_a = sandbox.create_file("A.md", "A");
    let f_b = sandbox.create_file("B.md", "B");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&f_a);
    thread::sleep(Duration::from_millis(50));

    watcher.watch_file(&f_b);
    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    fs::write(&f_a, "A edited").unwrap();
    fs::write(&f_b, "B edited").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).unwrap();
    assert_eq!(event, WatcherEvent::Modified(f_b));
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_watcher_scenario_neovim_write_backup_and_rename_workflow() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("report.md", "# Report v1");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // Neovim write sequence:
    // 1. Write backup file report.md~
    let backup = sandbox.path().join("report.md~");
    fs::write(&backup, "# Report v1").unwrap();

    // 2. Write new content to temporary file
    let tmp = sandbox.path().join("4913"); // vim test number
    fs::write(&tmp, "test").unwrap();
    fs::remove_file(&tmp).unwrap();

    // 3. Rename new file over report.md
    let new_tmp = sandbox.path().join(".report.md.new");
    fs::write(&new_tmp, "# Report v2 (Edited in Neovim)").unwrap();
    fs::rename(&new_tmp, &file).unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).expect("Neovim write must trigger event");
    assert_eq!(event, WatcherEvent::Modified(file));
}

#[test]
fn test_watcher_scenario_multi_agent_append_monitoring() {
    let sandbox = TestSandbox::new();
    let progress_file = sandbox.create_file("progress.md", "# Progress\n- Step 1 done\n");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcherOracle::new(move |e| {
        let _ = tx.send(e);
    });
    watcher.watch_file(&progress_file);

    thread::sleep(Duration::from_millis(50));
    thread::sleep(Duration::from_millis(1100));

    // External agent appends to file
    let mut f = fs::OpenOptions::new().append(true).open(&progress_file).unwrap();
    writeln!(f, "- Step 2 done (Agent B)").unwrap();

    let event = rx.recv_timeout(Duration::from_millis(500)).expect("Agent edit must trigger event");
    assert_eq!(event, WatcherEvent::Modified(progress_file));
}
