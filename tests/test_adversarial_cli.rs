//! Empirical Adversarial Stress Test Suite for Typora-RustReader CLI & Path Resolution.
//!
//! Directly tests `typora_rust_reader::cli` functions AND the compiled binary `target/release/typora-rust-reader`.

mod common;
use common::TestSandbox;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use typora_rust_reader::cli::*;

fn get_release_binary() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rel_bin = manifest_dir.join("target/release/typora-rust-reader");
    if rel_bin.exists() {
        return rel_bin;
    }
    let debug_bin = manifest_dir.join("target/debug/typora-rust-reader");
    if debug_bin.exists() {
        return debug_bin;
    }
    panic!("No compiled typora-rust-reader binary found!");
}

// =========================================================================
// 1. DEEPLY NESTED PATHS
// =========================================================================

#[test]
fn test_adv_deeply_nested_existing_path() {
    let sandbox = TestSandbox::new();
    let mut deep_rel = PathBuf::new();
    for i in 0..50 {
        deep_rel.push(format!("dir_{}", i));
    }
    let deep_dir = sandbox.path().join(&deep_rel);
    fs::create_dir_all(&deep_dir).unwrap();
    let target_file = deep_dir.join("deep_doc.md");
    fs::write(&target_file, "# Deep Content").unwrap();

    let res = resolve_initial_file(Some(deep_rel.join("deep_doc.md").to_str().unwrap()), sandbox.path()).unwrap();
    assert_eq!(res, Some(target_file.canonicalize().unwrap()));
}

#[test]
fn test_adv_deeply_nested_nonexistent_path() {
    let sandbox = TestSandbox::new();
    let mut deep_rel = PathBuf::new();
    for i in 0..50 {
        deep_rel.push(format!("missing_{}", i));
    }
    let target = deep_rel.join("missing.md");
    let res = resolve_initial_file(Some(target.to_str().unwrap()), sandbox.path());
    assert!(res.is_err(), "Expected error for nonexistent deep path, got {:?}", res);
    assert!(res.unwrap_err().contains("File not found"));
}

#[test]
fn test_adv_deeply_nested_directory_auto_discovery() {
    let sandbox = TestSandbox::new();
    let mut deep_rel = PathBuf::new();
    for i in 0..30 {
        deep_rel.push(format!("nested_{}", i));
    }
    let deep_dir = sandbox.path().join(&deep_rel);
    fs::create_dir_all(&deep_dir).unwrap();
    let target_file = deep_dir.join("index.md");
    fs::write(&target_file, "# Nested Index").unwrap();

    let res = resolve_initial_file(Some(deep_rel.to_str().unwrap()), sandbox.path()).unwrap();
    assert_eq!(res, Some(target_file.canonicalize().unwrap()));
}

// =========================================================================
// 2. WHITESPACE IN PATHS
// =========================================================================

#[test]
fn test_adv_path_with_internal_spaces() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("My Personal Notes 2026.md", "hello");
    let res = resolve_initial_file(Some("My Personal Notes 2026.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_adv_path_with_leading_or_trailing_whitespace_preserved() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("  trimmed.md  ", "hello");
    let res = resolve_initial_file(Some("  trimmed.md  "), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_adv_whitespace_only_argument_treated_literally() {
    let sandbox = TestSandbox::new();
    let _readme = sandbox.create_file("README.md", "hello");
    let res = resolve_initial_file(Some("   "), sandbox.path());
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("File not found:    "));
}

#[test]
fn test_adv_empty_string_argument_rejected() {
    let sandbox = TestSandbox::new();
    let _readme = sandbox.create_file("README.md", "hello");
    let res = resolve_initial_file(Some(""), sandbox.path());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Empty file path argument provided");
}

// =========================================================================
// 3. NONEXISTENT PATHS
// =========================================================================

#[test]
fn test_adv_nonexistent_file_returns_error() {
    let sandbox = TestSandbox::new();
    let res = resolve_initial_file(Some("does_not_exist_99.md"), sandbox.path());
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("File not found: does_not_exist_99.md"), "Got err: {}", err);
}

#[test]
fn test_adv_nonexistent_absolute_path_returns_error() {
    let sandbox = TestSandbox::new();
    let abs_missing = sandbox.path().join("completely_missing.md");
    let res = resolve_initial_file(Some(abs_missing.to_str().unwrap()), sandbox.path());
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("File not found"));
}

#[test]
fn test_adv_binary_nonexistent_file_exits_with_error_code() {
    let bin = get_release_binary();
    let output = Command::new(&bin)
        .arg("nonexistent_doc_never_created_12345.md")
        .output()
        .expect("Failed to run binary");
    assert!(!output.status.success(), "Binary should fail on missing file");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("File not found") || stderr.contains("nonexistent_doc_never_created_12345.md"),
        "Expected error message on stderr, got: {}",
        stderr
    );
}

// =========================================================================
// 4. RELATIVE TRAVERSALS (`../../../`)
// =========================================================================

#[test]
fn test_adv_relative_traversal_above_cwd_existing() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("target_above.md", "# Above");
    let sub = sandbox.path().join("sub1").join("sub2").join("sub3");
    fs::create_dir_all(&sub).unwrap();

    let res = resolve_initial_file(Some("../../../target_above.md"), &sub).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_adv_relative_traversal_above_cwd_nonexistent() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("sub1").join("sub2");
    fs::create_dir_all(&sub).unwrap();

    let res = resolve_initial_file(Some("../../missing.md"), &sub);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("File not found"));
}

#[test]
fn test_adv_redundant_dot_and_dotdot_components() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("norm.md", "# Normalized");
    // Redundant ./ components
    let res = resolve_initial_file(Some("././norm.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));

    // Redundant a/.. when directory 'a' exists
    let _dir_a = sandbox.path().join("a");
    fs::create_dir_all(&_dir_a).unwrap();
    let res = resolve_initial_file(Some("./a/.././norm.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_adv_redundant_dotdot_when_intermediate_dir_missing_behavior() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("norm.md", "# Normalized");
    let res = resolve_initial_file(Some("./nonexistent_intermediate/../norm.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_adv_leading_and_trailing_whitespace_stripping_discrepancy() {
    let sandbox = TestSandbox::new();
    let file_with_leading_space = sandbox.create_file(" doc_with_space.md", "content");
    assert!(file_with_leading_space.exists());

    let res = resolve_initial_file(Some(" doc_with_space.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(file_with_leading_space.canonicalize().unwrap()));
}

// =========================================================================
// 5. TILDE EXPANSIONS (`~`, `~/...`)
// =========================================================================

#[test]
fn test_adv_tilde_root_expansion() {
    let home = std::env::var_os("HOME").expect("HOME not set");
    let expanded = expand_tilde("~");
    assert_eq!(expanded, PathBuf::from(home));
}

#[test]
fn test_adv_tilde_slash_expansion() {
    let home = std::env::var_os("HOME").expect("HOME not set");
    let expanded = expand_tilde("~/docs/notes.md");
    assert_eq!(expanded, PathBuf::from(home).join("docs/notes.md"));
}

#[test]
fn test_adv_tilde_without_slash_non_home() {
    let expanded = expand_tilde("~alice/doc.md");
    // Standard Unix: non-root tilde without slash is preserved
    assert_eq!(expanded, PathBuf::from("~alice/doc.md"));
}

#[test]
fn test_adv_tilde_trailing_slash() {
    let home = std::env::var_os("HOME").expect("HOME not set");
    let expanded = expand_tilde("~/");
    assert_eq!(expanded, PathBuf::from(home));
}

#[test]
fn test_adv_tilde_in_middle_not_expanded() {
    let expanded = expand_tilde("some/path/~/file.md");
    assert_eq!(expanded, PathBuf::from("some/path/~/file.md"));
}

// =========================================================================
// 6. INVALID `--mode` AND `--theme` FLAGS
// =========================================================================

#[test]
fn test_adv_mode_flag_parsing_and_normalization() {
    // Valid values
    let args = parse_cli_from(&["typora", "-m", "read"]).unwrap();
    assert_eq!(args.normalized_mode(), "read");
    let args = parse_cli_from(&["typora", "--mode", "split"]).unwrap();
    assert_eq!(args.normalized_mode(), "split");
    let args = parse_cli_from(&["typora", "-m", "edit"]).unwrap();
    assert_eq!(args.normalized_mode(), "edit");

    // Synonyms and case-insensitivity
    let args = parse_cli_from(&["typora", "-m", "PREVIEW"]).unwrap();
    assert_eq!(args.normalized_mode(), "read");
    let args = parse_cli_from(&["typora", "-m", "Editor"]).unwrap();
    assert_eq!(args.normalized_mode(), "edit");

    // Invalid mode values fall back to "split"
    let args = parse_cli_from(&["typora", "-m", "invalid_mode_value"]).unwrap();
    assert_eq!(args.normalized_mode(), "split");
    let args = parse_cli_from(&["typora", "-m", "12345"]).unwrap();
    assert_eq!(args.normalized_mode(), "split");
}

#[test]
fn test_adv_theme_flag_parsing_and_normalization() {
    // Valid values
    for theme in &["github", "night", "newsprint", "gothic"] {
        let args = parse_cli_from(&["typora", "-t", theme]).unwrap();
        assert_eq!(args.normalized_theme(), *theme);
    }

    // Case-insensitivity & whitespace trimming
    let args = parse_cli_from(&["typora", "-t", " NIGHT "]).unwrap();
    assert_eq!(args.normalized_theme(), "night");

    // Invalid theme values fall back to "github"
    let args = parse_cli_from(&["typora", "-t", "solarized_dark"]).unwrap();
    assert_eq!(args.normalized_theme(), "github");
    let args = parse_cli_from(&["typora", "--theme", "monokai"]).unwrap();
    assert_eq!(args.normalized_theme(), "github");
}

#[test]
fn test_adv_missing_flag_arguments_rejected_by_clap() {
    // Missing argument to --mode
    let err_mode = parse_cli_from(&["typora", "--mode"]);
    assert!(err_mode.is_err(), "Expected clap error for --mode without arg");

    // Missing argument to --theme
    let err_theme = parse_cli_from(&["typora", "--theme"]);
    assert!(err_theme.is_err(), "Expected clap error for --theme without arg");

    // Missing argument to short flags
    let err_m = parse_cli_from(&["typora", "-m"]);
    assert!(err_m.is_err());
    let err_t = parse_cli_from(&["typora", "-t"]);
    assert!(err_t.is_err());
}

#[test]
fn test_adv_unknown_flags_rejected_by_clap() {
    let err = parse_cli_from(&["typora", "--unknown-flag"]);
    assert!(err.is_err());

    let err_short = parse_cli_from(&["typora", "-z"]);
    assert!(err_short.is_err());
}

#[test]
fn test_adv_multiple_positional_arguments_rejected_by_clap() {
    let err = parse_cli_from(&["typora", "doc1.md", "doc2.md"]);
    assert!(err.is_err());
}

// =========================================================================
// 7. EMPTY DIRECTORY AUTO-DISCOVERY
// =========================================================================

#[test]
fn test_adv_auto_discover_in_empty_directory_returns_none() {
    let sandbox = TestSandbox::new();
    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, None, "Auto-discovery in empty directory must return None (untitled scratchpad)");
}

#[test]
fn test_adv_explicit_empty_directory_argument_returns_error() {
    let sandbox = TestSandbox::new();
    let res = resolve_initial_file(Some(sandbox.path().to_str().unwrap()), sandbox.path());
    assert!(res.is_err(), "Passing an explicit empty directory must return error");
    let err = res.unwrap_err();
    assert!(err.contains("Directory contains no markdown files"), "Got: {}", err);
}

// =========================================================================
// 8. DIRECTORIES WITH NON-MARKDOWN FILES
// =========================================================================

#[test]
fn test_adv_directory_with_only_non_markdown_files_returns_none_in_cwd() {
    let sandbox = TestSandbox::new();
    sandbox.create_file("document.txt", "Text file");
    sandbox.create_file("image.png", "fake png");
    sandbox.create_file("data.json", "{}");
    sandbox.create_file("build.rs", "fn main() {}");

    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, None, "Directory with no markdown files must return None on CWD discovery");
}

#[test]
fn test_adv_explicit_directory_with_only_non_markdown_returns_error() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("assets_only");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("logo.svg"), "<svg></svg>").unwrap();
    fs::write(sub.join("styles.css"), "body {}").unwrap();

    let res = resolve_initial_file(Some("assets_only"), sandbox.path());
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Directory contains no markdown files"));
}

#[test]
fn test_adv_hidden_markdown_files_are_ignored() {
    let sandbox = TestSandbox::new();
    sandbox.create_file(".secret.md", "secret");
    sandbox.create_file(".hidden.markdown", "hidden");

    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, None, "Hidden markdown files starting with '.' must be ignored in auto-discovery");
}

#[test]
fn test_adv_readme_md_takes_precedence_over_alphabetical_and_index() {
    let sandbox = TestSandbox::new();
    sandbox.create_file("00_first.md", "# First");
    sandbox.create_file("index.md", "# Index");
    let readme = sandbox.create_file("README.md", "# Readme");

    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, Some(readme.canonicalize().unwrap()));
}

#[test]
fn test_adv_readme_case_variations() {
    for name in &["readme.md", "Readme.md"] {
        let sandbox = TestSandbox::new();
        sandbox.create_file("other.md", "# Other");
        let readme = sandbox.create_file(name, "# Readme");
        let res = resolve_initial_file(None, sandbox.path()).unwrap();
        assert_eq!(res, Some(readme.canonicalize().unwrap()));
    }
}

#[test]
fn test_adv_index_md_takes_precedence_over_other_markdown() {
    let sandbox = TestSandbox::new();
    sandbox.create_file("01_alpha.md", "# Alpha");
    let index = sandbox.create_file("index.md", "# Index");

    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, Some(index.canonicalize().unwrap()));
}

#[test]
fn test_adv_alphabetical_first_when_multiple_regular_markdown_files() {
    let sandbox = TestSandbox::new();
    sandbox.create_file("zebra.md", "# Zebra");
    let beta = sandbox.create_file("beta.md", "# Beta");
    sandbox.create_file("charlie.markdown", "# Charlie");

    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, Some(beta.canonicalize().unwrap()));
}

#[test]
fn test_adv_uppercase_markdown_extensions_recognized() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("NOTES.MD", "# Uppercase");

    let res = resolve_initial_file(None, sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

// =========================================================================
// 9. SPECIAL "HELP" KEYWORD & CASE INSENSITIVITY
// =========================================================================

#[test]
fn test_adv_help_keyword_case_insensitive() {
    let sandbox = TestSandbox::new();
    for kw in &["help", "HELP", "Help", "hElP"] {
        let res = resolve_initial_file(Some(kw), sandbox.path()).unwrap();
        assert!(res.is_some());
        let path = res.unwrap();
        assert!(path.exists(), "Help target {:?} must exist", path);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Typora") || content.contains("Markdown") || content.contains("Shortcuts") || content.contains("Help"));
    }
}

// =========================================================================
// 10. SYMBOLIC LINKS & SPECIAL CASES
// =========================================================================

#[test]
fn test_adv_symlinked_markdown_file_resolves() {
    let sandbox = TestSandbox::new();
    let real_file = sandbox.create_file("real.md", "# Real");
    let symlink = sandbox.path().join("link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_file, &symlink).unwrap();
        let res = resolve_initial_file(Some("link.md"), sandbox.path()).unwrap();
        assert_eq!(res, Some(real_file.canonicalize().unwrap()));
    }
}

#[test]
fn test_adv_dangling_symlink_returns_error() {
    let sandbox = TestSandbox::new();
    let dangling = sandbox.path().join("dangling.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(sandbox.path().join("nonexistent_target.md"), &dangling).unwrap();
        let res = resolve_initial_file(Some("dangling.md"), sandbox.path());
        assert!(res.is_err(), "Dangling symlink should fail resolution");
    }
}

// =========================================================================
// 11. COMPILED BINARY E2E CLI INVOCATIONS
// =========================================================================

#[test]
fn test_adv_binary_help_flag_stdout() {
    let bin = get_release_binary();
    let output = Command::new(&bin).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: typora-rust-reader"));
    assert!(stdout.contains("-m, --mode"));
    assert!(stdout.contains("-t, --theme"));
}

#[test]
fn test_adv_binary_version_flag_stdout() {
    let bin = get_release_binary();
    let output = Command::new(&bin).arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("typora-rust-reader"));
}

#[test]
fn test_adv_binary_missing_mode_arg_fails() {
    let bin = get_release_binary();
    let output = Command::new(&bin).arg("--mode").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_adv_binary_missing_theme_arg_fails() {
    let bin = get_release_binary();
    let output = Command::new(&bin).arg("--theme").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_adv_binary_unknown_flag_fails() {
    let bin = get_release_binary();
    let output = Command::new(&bin).arg("--bogus-flag").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
