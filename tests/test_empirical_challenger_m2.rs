//! Empirical Challenger Stress Tests for Milestone 2 Gate Verification
//!
//! Specifically stress tests Kernel File Watcher (`src/watcher.rs`) and File I/O (`src/io.rs`):
//! 1. Burst external writes: rapidly write 20 times within 40ms -> <= 2 callbacks.
//! 2. Self-save suppression: call atomic_write_file, then mark_self_saved(mtime) -> reload suppressed.
//! 3. Later external edit after self-save: verify subsequent external edit IS detected and fires callback.
//! 4. Atomic file replacement: simulate external editors (e.g. Neovim temp file write + backup + rename).
//! 5. Dynamic re-targeting: switch watch_file to new document -> old events discarded, new detected.
//! 6. Dynamic re-targeting in same directory: verify old file ignored, new file detected.
//! 7. High-frequency write stress (100 rapid writes): verify debouncer stability.
//! 8. External deletion and recreation: verify Deleted event followed by Modified event.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use typora_rust_reader::io::atomic_write_file;
use typora_rust_reader::watcher::{DocumentWatcher, WatcherEvent};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("typora_challenger_m2_{}_{}_{}", prefix, pid, nanos));
        fs::create_dir_all(&path).expect("Failed to create test sandbox directory");
        Self { path }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn create_file(&self, name: &str, content: &str) -> PathBuf {
        let p = self.file_path(name);
        let mut f = File::create(&p).expect("Failed to create test file");
        f.write_all(content.as_bytes()).expect("Failed to write initial content");
        f.sync_all().expect("Failed to sync initial file");
        p
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_challenge_burst_external_writes_collapses_to_at_most_two_callbacks() {
    let sandbox = TestDir::new("burst");
    let file = sandbox.create_file("burst.md", "0");

    let received_count = Arc::new(AtomicU64::new(0));
    let count_clone = Arc::clone(&received_count);

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        count_clone.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).expect("Failed to watch file");
    thread::sleep(Duration::from_millis(60));

    // Rapidly perform 20 external writes within ~40ms
    let start = Instant::now();
    for i in 1..=20 {
        fs::write(&file, format!("Write payload {}", i)).expect("Write failed");
        thread::sleep(Duration::from_millis(2));
    }
    let elapsed = start.elapsed();
    println!("20 writes completed in {:?}", elapsed);

    // Wait for the 50ms debouncer window to settle
    thread::sleep(Duration::from_millis(300));

    let total = received_count.load(Ordering::SeqCst);
    println!("Total callbacks received: {}", total);

    assert!(
        total >= 1 && total <= 2,
        "50ms debouncer must collapse 20 rapid burst writes into <= 2 callbacks, got {}",
        total
    );

    // Drain channel and verify events are Modified
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    assert_eq!(events.len(), total as usize);
    for ev in events {
        assert_eq!(ev, WatcherEvent::Modified(file.clone()));
    }
}

#[test]
fn test_challenge_self_save_suppression() {
    let sandbox = TestDir::new("self_save");
    let file = sandbox.create_file("doc.md", "Initial text");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).expect("Failed to watch file");
    thread::sleep(Duration::from_millis(60));

    // Ensure mtime advances past initial creation
    thread::sleep(Duration::from_millis(1100));

    // Execute atomic_write_file and immediately mark self saved
    let new_mtime = atomic_write_file(&file, "Saved internally by editor")
        .expect("atomic_write_file failed");
    watcher.mark_self_saved(new_mtime);

    // Wait for any inotify event to arrive and debouncer to expire
    let res = rx.recv_timeout(Duration::from_millis(400));
    assert!(
        res.is_err(),
        "Watcher must suppress reload event when mtime matches mark_self_saved ({:?})",
        res
    );
}

#[test]
fn test_challenge_later_external_edit_after_self_save() {
    let sandbox = TestDir::new("later_edit");
    let file = sandbox.create_file("multi_step.md", "v1.0");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).expect("Failed to watch file");
    thread::sleep(Duration::from_millis(60));

    // Phase 1: Internal self-save
    thread::sleep(Duration::from_millis(1100));
    let internal_mtime = atomic_write_file(&file, "v1.1 (Internal Save)")
        .expect("atomic_write_file failed");
    watcher.mark_self_saved(internal_mtime);

    // Verify self-save is suppressed
    thread::sleep(Duration::from_millis(250));
    assert!(rx.try_recv().is_err(), "Self-save event should have been suppressed");

    // Phase 2: External edit after self-save (advance 1-second mtime resolution)
    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "v2.0 (External Modification by outside process)")
        .expect("External write failed");

    // Verify subsequent external edit IS detected
    let ev = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("External edit after self-save MUST be detected");
    assert_eq!(ev, WatcherEvent::Modified(file));
}

#[test]
fn test_challenge_atomic_file_replacement_neovim_simulation() {
    let sandbox = TestDir::new("neovim");
    let file = sandbox.create_file("code.md", "# Original Content");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).expect("Failed to watch file");
    thread::sleep(Duration::from_millis(60));
    thread::sleep(Duration::from_millis(1100));

    // Simulate Neovim write pattern:
    // 1. Write backup file (.md~)
    let backup_file = sandbox.file_path("code.md~");
    fs::write(&backup_file, "# Original Content Backup").expect("Write backup failed");

    // 2. Write and delete vim test file "4913"
    let probe_file = sandbox.file_path("4913");
    fs::write(&probe_file, "probe").expect("Write probe failed");
    fs::remove_file(&probe_file).expect("Remove probe failed");

    // 3. Write new content into hidden temp file
    let tmp_file = sandbox.file_path(".code.md.new");
    fs::write(&tmp_file, "# Neovim Updated Content").expect("Write temp failed");

    // 4. Rename temp file over target file (atomic inode replacement)
    fs::rename(&tmp_file, &file).expect("Rename failed");

    // Verify rename is detected by parent-directory watcher
    let ev = rx
        .recv_timeout(Duration::from_millis(600))
        .expect("Atomic file replacement via rename MUST be detected");
    assert_eq!(ev, WatcherEvent::Modified(file));
}

#[test]
fn test_challenge_dynamic_retargeting_different_directory() {
    let dir1 = TestDir::new("retarget_dir1");
    let dir2 = TestDir::new("retarget_dir2");

    let file1 = dir1.create_file("doc1.md", "Content in dir1");
    let file2 = dir2.create_file("doc2.md", "Content in dir2");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    // 1. Initially watch file1 in dir1
    watcher.watch_file(&file1).expect("Failed to watch file1");
    thread::sleep(Duration::from_millis(60));

    // 2. Dynamically switch to watch file2 in dir2
    watcher.watch_file(&file2).expect("Failed to watch file2");
    thread::sleep(Duration::from_millis(60));
    thread::sleep(Duration::from_millis(1100));

    // 3. Modify old file1 in dir1 -> events must be discarded
    fs::write(&file1, "Modified file1").expect("Write file1 failed");
    thread::sleep(Duration::from_millis(150));
    assert!(
        rx.try_recv().is_err(),
        "Events for un-targeted old document in old directory must be discarded"
    );

    // 4. Modify new file2 in dir2 -> event MUST be detected
    fs::write(&file2, "Modified file2").expect("Write file2 failed");
    let ev = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("Event for newly targeted document in dir2 MUST be received");
    assert_eq!(ev, WatcherEvent::Modified(file2));
}

#[test]
fn test_challenge_dynamic_retargeting_same_directory() {
    let sandbox = TestDir::new("retarget_same_dir");
    let file_a = sandbox.create_file("alpha.md", "Alpha");
    let file_b = sandbox.create_file("beta.md", "Beta");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    // Watch alpha.md first
    watcher.watch_file(&file_a).expect("Failed to watch alpha");
    thread::sleep(Duration::from_millis(60));

    // Switch to beta.md
    watcher.watch_file(&file_b).expect("Failed to watch beta");
    thread::sleep(Duration::from_millis(60));
    thread::sleep(Duration::from_millis(1100));

    // Write to alpha.md -> must NOT trigger
    fs::write(&file_a, "Alpha modified").expect("Write alpha failed");
    thread::sleep(Duration::from_millis(150));
    assert!(rx.try_recv().is_err(), "Events for alpha.md must be ignored after switching to beta.md");

    // Write to beta.md -> MUST trigger
    fs::write(&file_b, "Beta modified").expect("Write beta failed");
    let ev = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("Events for beta.md MUST trigger after re-targeting");
    assert_eq!(ev, WatcherEvent::Modified(file_b));
}

#[test]
fn test_challenge_high_frequency_burst_stress_100_writes() {
    let sandbox = TestDir::new("stress_100");
    let file = sandbox.create_file("stress.md", "Initial");

    let received_count = Arc::new(AtomicU64::new(0));
    let count_clone = Arc::clone(&received_count);

    let (tx, _rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        count_clone.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).expect("Failed to watch file");
    thread::sleep(Duration::from_millis(60));

    // 100 rapid writes
    for i in 1..=100 {
        fs::write(&file, format!("Stress payload write number {}", i)).expect("Write failed");
        if i % 10 == 0 {
            thread::sleep(Duration::from_millis(1));
        }
    }

    // Wait for debouncer to settle
    thread::sleep(Duration::from_millis(300));

    let total = received_count.load(Ordering::SeqCst);
    println!("100 rapid writes resulted in {} callbacks", total);

    assert!(
        total >= 1 && total <= 2,
        "100 rapid writes must be collapsed into <= 2 callbacks, got {}",
        total
    );
}

#[test]
fn test_challenge_deletion_and_recreation() {
    let sandbox = TestDir::new("del_recreate");
    let file = sandbox.create_file("lifecycle.md", "Alive");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        println!("DOCUMENT WATCHER CALLBACK: {:?}", ev);
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).expect("Failed to watch file");
    thread::sleep(Duration::from_millis(200));

    println!("Now removing file: {:?}", file);
    fs::remove_file(&file).expect("Failed to remove file");

    let del_ev = rx.recv_timeout(Duration::from_millis(2000));
    println!("Del ev result: {:?}", del_ev);
    assert!(del_ev.is_ok(), "Must receive delete event within 2s, got: {:?}", del_ev);
    assert_eq!(del_ev.unwrap(), WatcherEvent::Deleted(file.clone()));

    thread::sleep(Duration::from_millis(1100));
    println!("Now recreating file: {:?}", file);
    fs::write(&file, "Resurrected").expect("Failed to recreate file");

    let mod_ev = rx.recv_timeout(Duration::from_millis(2000));
    println!("Mod ev result: {:?}", mod_ev);
    assert!(mod_ev.is_ok(), "Must receive mod event on recreation, got: {:?}", mod_ev);
    assert_eq!(mod_ev.unwrap(), WatcherEvent::Modified(file));
}
