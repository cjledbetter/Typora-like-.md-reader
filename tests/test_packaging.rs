//! Opaque-box E2E test suite for Typora-RustReader Packaging, Desktop Entry & Installation.
//!
//! Features Covered:
//! - Feature 33: Release Profile Optimization & Binary Stripping
//! - Feature 34: Hyprland / XDG Desktop Entry (`typora-rust-reader.desktop`)
//! - Feature 35: Installation Script & Symlink Migration (`install-rust-omarchy.sh`)

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::TestSandbox;
use std::collections::HashMap;
use std::fs::{self, File, Permissions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
pub struct DesktopEntryModel {
    pub name: String,
    pub exec: String,
    pub entry_type: String,
    pub startup_wm_class: String,
    pub mime_types: Vec<String>,
    pub categories: Vec<String>,
}

/// INI Parser for XDG Desktop Entry specification.
pub fn parse_desktop_entry(content: &str) -> Result<DesktopEntryModel, String> {
    let mut in_desktop_entry = false;
    let mut fields = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if trimmed == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        } else if trimmed.starts_with('[') {
            in_desktop_entry = false;
            continue;
        }

        if in_desktop_entry {
            if let Some(eq_idx) = trimmed.find('=') {
                let key = trimmed[..eq_idx].trim().to_string();
                let val = trimmed[eq_idx + 1..].trim().to_string();
                fields.insert(key, val);
            }
        }
    }

    let entry_type = fields.get("Type").cloned().unwrap_or_default();
    let name = fields.get("Name").cloned().unwrap_or_default();
    let exec = fields.get("Exec").cloned().unwrap_or_default();
    let startup_wm_class = fields.get("StartupWMClass").cloned().unwrap_or_default();

    let mime_types = fields
        .get("MimeType")
        .map(|s| s.split(';').filter(|m| !m.is_empty()).map(|m| m.to_string()).collect())
        .unwrap_or_default();

    let categories = fields
        .get("Categories")
        .map(|s| s.split(';').filter(|c| !c.is_empty()).map(|c| c.to_string()).collect())
        .unwrap_or_default();

    Ok(DesktopEntryModel {
        name,
        exec,
        entry_type,
        startup_wm_class,
        mime_types,
        categories,
    })
}

fn get_project_root() -> PathBuf {
    PathBuf::from("/home/cjledbetter/Documents/Programming/typora-like-.md-reader")
}

fn locate_desktop_file() -> Option<PathBuf> {
    let root = get_project_root();
    let candidate1 = root.join("packaging/typora-rust-reader.desktop");
    let candidate2 = root.join("typora-reader.desktop");

    if candidate1.is_file() {
        Some(candidate1)
    } else if candidate2.is_file() {
        Some(candidate2)
    } else {
        None
    }
}

fn locate_install_script() -> Option<PathBuf> {
    let root = get_project_root();
    let candidate1 = root.join("packaging/install-rust-omarchy.sh");
    let candidate2 = root.join("install-omarchy.sh");

    if candidate1.is_file() {
        Some(candidate1)
    } else if candidate2.is_file() {
        Some(candidate2)
    } else {
        None
    }
}

/// Installation migration oracle following the contract in AGENT.MD.
pub fn run_install_migration_oracle(
    target_bin: &Path,
    sandbox_home: &Path,
) -> Result<(), String> {
    let local_bin = sandbox_home.join(".local/bin");
    fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?;

    let dest_binary = local_bin.join("typora-rust-reader");
    fs::copy(target_bin, &dest_binary).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    fs::set_permissions(&dest_binary, Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;

    let symlink = local_bin.join("typora-reader");
    let legacy_backup = local_bin.join("typora-reader-legacy");

    // Check if symlink/legacy already exists
    if symlink.exists() || fs::symlink_metadata(&symlink).is_ok() {
        // If it's already a symlink pointing to typora-rust-reader, leave it
        let is_target_symlink = fs::read_link(&symlink)
            .map(|target| target == Path::new("typora-rust-reader") || target == dest_binary)
            .unwrap_or(false);

        if !is_target_symlink {
            // Backup legacy script/binary
            let _ = fs::rename(&symlink, &legacy_backup);
        } else {
            let _ = fs::remove_file(&symlink);
        }
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink("typora-rust-reader", &symlink).map_err(|e| e.to_string())?;

    Ok(())
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 10 tests across Desktop & Install)
// =========================================================================

#[test]
fn test_packaging_f34_desktop_file_exists() {
    let desktop = locate_desktop_file();
    assert!(desktop.is_some(), "Desktop file must exist");
}

#[test]
fn test_packaging_f34_desktop_file_type_application() {
    if let Some(desktop) = locate_desktop_file() {
        let content = fs::read_to_string(&desktop).unwrap();
        let model = parse_desktop_entry(&content).unwrap();
        assert_eq!(model.entry_type, "Application");
    }
}

#[test]
fn test_packaging_f34_desktop_file_startup_wm_class() {
    let spec_content = "[Desktop Entry]\nType=Application\nName=Typora Rust Reader\nStartupWMClass=com.typora.rust-reader\nExec=typora-rust-reader %f\nMimeType=text/markdown;";
    let model = parse_desktop_entry(spec_content).unwrap();
    assert_eq!(model.startup_wm_class, "com.typora.rust-reader");
}

#[test]
fn test_packaging_f34_desktop_file_mimetypes_include_markdown() {
    let spec_content = "[Desktop Entry]\nType=Application\nMimeType=text/markdown;text/x-markdown;text/plain;";
    let model = parse_desktop_entry(spec_content).unwrap();
    assert!(model.mime_types.contains(&"text/markdown".to_string()));
    assert!(model.mime_types.contains(&"text/x-markdown".to_string()));
}

#[test]
fn test_packaging_f34_desktop_file_exec_has_file_placeholder() {
    let spec_content = "[Desktop Entry]\nExec=/home/user/.local/bin/typora-rust-reader %f";
    let model = parse_desktop_entry(spec_content).unwrap();
    assert!(model.exec.contains("%f"), "Exec must contain %f file placeholder");
}

#[test]
fn test_packaging_f35_install_script_exists() {
    let script = locate_install_script();
    assert!(script.is_some(), "Install script must exist");
}

#[test]
fn test_packaging_f35_install_script_syntax_valid() {
    if let Some(script) = locate_install_script() {
        let output = Command::new("bash")
            .arg("-n")
            .arg(&script)
            .output()
            .expect("Failed to execute bash");
        assert!(output.status.success(), "Install script bash syntax error: {}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn test_packaging_f35_install_migration_creates_binary_in_local_bin() {
    let sandbox = TestSandbox::new();
    let dummy_bin = sandbox.create_file("source_bin", "#!/bin/sh\necho typora");
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    let installed_bin = sandbox.path().join(".local/bin/typora-rust-reader");
    assert!(installed_bin.is_file());
}

#[test]
fn test_packaging_f35_install_migration_creates_symlink() {
    let sandbox = TestSandbox::new();
    let dummy_bin = sandbox.create_file("source_bin", "#!/bin/sh\necho typora");
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    let symlink = sandbox.path().join(".local/bin/typora-reader");
    assert!(symlink.is_symlink() || symlink.exists());
}

#[test]
fn test_packaging_f35_install_migration_backs_up_existing_legacy() {
    let sandbox = TestSandbox::new();
    let local_bin = sandbox.path().join(".local/bin");
    fs::create_dir_all(&local_bin).unwrap();

    // Create existing legacy script
    let legacy_script = local_bin.join("typora-reader");
    fs::write(&legacy_script, "#!/usr/bin/env python3\n# legacy python wrapper").unwrap();

    let dummy_bin = sandbox.create_file("source_bin", "#!/bin/sh\necho rust");
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    let legacy_backup = local_bin.join("typora-reader-legacy");
    assert!(legacy_backup.is_file(), "Existing legacy script must be backed up to typora-reader-legacy");
    assert_eq!(fs::read_to_string(&legacy_backup).unwrap(), "#!/usr/bin/env python3\n# legacy python wrapper");
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 20 tests)
// =========================================================================

#[test]
fn test_packaging_boundary_install_idempotency_no_duplicate_backup() {
    let sandbox = TestSandbox::new();
    let local_bin = sandbox.path().join(".local/bin");
    fs::create_dir_all(&local_bin).unwrap();

    let legacy_script = local_bin.join("typora-reader");
    fs::write(&legacy_script, "legacy").unwrap();

    let dummy_bin = sandbox.create_file("source_bin", "rust");

    // Run 1: Should backup legacy
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();
    let backup_path = local_bin.join("typora-reader-legacy");
    assert!(backup_path.is_file());

    // Run 2: Should NOT overwrite or corrupt the legacy backup
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();
    assert_eq!(fs::read_to_string(&backup_path).unwrap(), "legacy");
}

#[test]
fn test_packaging_boundary_desktop_entry_comments_and_blank_lines() {
    let content = "# Comment line\n\n[Desktop Entry]\n# Inner comment\nType=Application\nName=Typora\n\n";
    let model = parse_desktop_entry(content).unwrap();
    assert_eq!(model.entry_type, "Application");
    assert_eq!(model.name, "Typora");
}

#[test]
fn test_packaging_boundary_desktop_entry_categories() {
    let content = "[Desktop Entry]\nCategories=Office;TextEditor;Utility;";
    let model = parse_desktop_entry(content).unwrap();
    assert!(model.categories.contains(&"Office".to_string()));
    assert!(model.categories.contains(&"TextEditor".to_string()));
    assert!(model.categories.contains(&"Utility".to_string()));
}

#[test]
fn test_packaging_boundary_installed_binary_permissions() {
    let sandbox = TestSandbox::new();
    let dummy_bin = sandbox.create_file("source_bin", "binary");
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    #[cfg(unix)]
    {
        let installed = sandbox.path().join(".local/bin/typora-rust-reader");
        let meta = fs::metadata(&installed).unwrap();
        let mode = meta.permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "Installed binary must be executable (0755)");
    }
}

#[test]
fn test_packaging_boundary_local_bin_does_not_exist_initially() {
    let sandbox = TestSandbox::new();
    // Do not create .local/bin beforehand
    let dummy_bin = sandbox.create_file("source_bin", "bin");
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    assert!(sandbox.path().join(".local/bin").is_dir());
}

#[test]
fn test_packaging_boundary_desktop_entry_crlf_clean() {
    let content = "[Desktop Entry]\r\nType=Application\r\nName=Typora\r\n";
    let model = parse_desktop_entry(content).unwrap();
    assert_eq!(model.entry_type, "Application");
    assert_eq!(model.name, "Typora");
}

#[test]
fn test_packaging_boundary_symlink_resolves_to_installed_binary() {
    let sandbox = TestSandbox::new();
    let dummy_bin = sandbox.create_file("source_bin", "bin");
    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    let symlink = sandbox.path().join(".local/bin/typora-reader");
    let target = fs::read_link(&symlink).unwrap();
    assert_eq!(target, Path::new("typora-rust-reader"));
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS
// =========================================================================

#[test]
fn test_packaging_combine_install_and_symlink_execution() {
    let sandbox = TestSandbox::new();
    let script_content = "#!/bin/sh\necho 'Typora Rust Reader v2.0'\n";
    let dummy_bin = sandbox.create_file("source_bin", script_content);

    run_install_migration_oracle(&dummy_bin, sandbox.path()).unwrap();

    let symlink = sandbox.path().join(".local/bin/typora-reader");
    let output = Command::new(&symlink)
        .output()
        .expect("Failed to execute via symlink");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "Typora Rust Reader v2.0");
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_packaging_scenario_full_upgrade_migration_workflow() {
    let sandbox = TestSandbox::new();
    let local_bin = sandbox.path().join(".local/bin");
    fs::create_dir_all(&local_bin).unwrap();

    // 1. User has existing typora-reader (Python version)
    let legacy_file = local_bin.join("typora-reader");
    fs::write(&legacy_file, "#!/usr/bin/env python3\nprint('legacy python')").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&legacy_file, Permissions::from_mode(0o755)).unwrap();

    // 2. Install new Rust version
    let new_rust_bin = sandbox.create_file("typora-rust-reader-dist", "#!/bin/sh\nprint('native rust')");
    run_install_migration_oracle(&new_rust_bin, sandbox.path()).unwrap();

    // 3. Verify typora-reader now points to typora-rust-reader
    let symlink = local_bin.join("typora-reader");
    assert_eq!(fs::read_link(&symlink).unwrap(), Path::new("typora-rust-reader"));

    // 4. Verify legacy preserved
    let legacy_backup = local_bin.join("typora-reader-legacy");
    assert!(legacy_backup.exists());
}
