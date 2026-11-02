//! Comprehensive Challenger 1 Empirical CLI & Path Resolution Stress Test Suite.
//!
//! Validates:
//! 1. Whitespace paths (leading, trailing, interior, tabs)
//! 2. Empty string rejection (code 1, specific error message)
//! 3. Complex relative paths with non-existent parent segments
//! 4. Unicode paths (CJK, Arabic, Emojis, Accents)
//! 5. Symlinks (direct, chained, directory, dangling, cyclic)
//! 6. Mode flags parsing and normalization
//! 7. Theme flags parsing and normalization

mod common;
use common::TestSandbox;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use typora_rust_reader::cli::*;

// =========================================================================
// 1. WHITESPACE PATHS
// =========================================================================

#[test]
fn test_challenger_leading_whitespace_preserved() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file(" leading_whitespace.md", "# Leading Space");
    let res = resolve_initial_file(Some(" leading_whitespace.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_challenger_trailing_whitespace_preserved() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("trailing_whitespace.md ", "# Trailing Space");
    let res = resolve_initial_file(Some("trailing_whitespace.md "), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_challenger_both_leading_and_trailing_whitespace_preserved() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("   both_spaces.md   ", "# Both Spaces");
    let res = resolve_initial_file(Some("   both_spaces.md   "), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_challenger_multiple_consecutive_interior_spaces() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("multiple   interior   spaces.md", "# Interior Spaces");
    let res = resolve_initial_file(Some("multiple   interior   spaces.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_challenger_tab_character_in_path() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("tab\tin\tpath.md", "# Tab Path");
    let res = resolve_initial_file(Some("tab\tin\tpath.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_challenger_whitespace_only_string_fails_without_autodiscovery() {
    let sandbox = TestSandbox::new();
    // Create README so if auto-discovery accidentally fired, it would match
    sandbox.create_file("README.md", "# Readme");
    let res = resolve_initial_file(Some("    "), sandbox.path());
    assert!(res.is_err(), "Whitespace-only arg must return Err, not match README");
    assert!(res.unwrap_err().contains("File not found:     "));
}

// =========================================================================
// 2. EMPTY STRING ARGUMENTS
// =========================================================================

#[test]
fn test_challenger_empty_string_returns_exact_error() {
    let sandbox = TestSandbox::new();
    sandbox.create_file("README.md", "# Readme");
    let res = resolve_initial_file(Some(""), sandbox.path());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Empty file path argument provided");
}

// =========================================================================
// 3. COMPLEX RELATIVE PATHS WITH NON-EXISTENT PARENT SEGMENTS
// =========================================================================

#[test]
fn test_challenger_nonexistent_parent_single_segment() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("real.md", "# Real");
    let res = resolve_initial_file(Some("./nonexistent_parent/../real.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_nonexistent_parent_multi_segments() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("real.md", "# Real");
    let res = resolve_initial_file(Some("./fake1/fake2/fake3/../../../real.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_redundant_dots_with_nonexistent_segment() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("real.md", "# Real");
    let res = resolve_initial_file(Some("./././fake/../././real.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_subfolder_with_nonexistent_sibling_segment() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("sub");
    fs::create_dir_all(&sub).unwrap();
    let target = sub.join("sub_real.md");
    File::create(&target).unwrap();

    let res = resolve_initial_file(Some("sub/nonexistent_fake/../sub_real.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_deep_10_level_nonexistent_segment_chain() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("deep_target.md", "# Deep Target");
    let path_str = "./d1/d2/d3/d4/d5/d6/d7/d8/d9/d10/../../../../../../../../../../deep_target.md";
    let res = resolve_initial_file(Some(path_str), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_nonexistent_file_via_nonexistent_segment_fails() {
    let sandbox = TestSandbox::new();
    let res = resolve_initial_file(Some("./fake/../definitely_missing.md"), sandbox.path());
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("File not found: ./fake/../definitely_missing.md"));
}

#[test]
fn test_challenger_normalize_path_root_traversal_boundary() {
    let path = Path::new("/../../../etc/passwd");
    let normalized = normalize_path(path);
    assert_eq!(normalized, PathBuf::from("/etc/passwd"));
}

// =========================================================================
// 4. UNICODE PATHS, SYMLINKS, AND NONEXISTENT PATHS
// =========================================================================

#[test]
fn test_challenger_unicode_cjk() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("中文测试文档_2026.md", "# 中文");
    let res = resolve_initial_file(Some("中文测试文档_2026.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_unicode_japanese() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("日本語ノート.md", "# 日本語");
    let res = resolve_initial_file(Some("日本語ノート.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_unicode_arabic_rtl() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("مستند_تجريبي.md", "# عربي");
    let res = resolve_initial_file(Some("مستند_تجريبي.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_unicode_emojis() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("🚀_rocket_doc_📝.md", "# Emojis");
    let res = resolve_initial_file(Some("🚀_rocket_doc_📝.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_unicode_accents_and_umlauts() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("café_über_naïve.md", "# Accents");
    let res = resolve_initial_file(Some("café_über_naïve.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_unicode_with_nonexistent_parent_and_spaces() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("   文档 🚀.md", "# Complex Unicode");
    let res = resolve_initial_file(Some("./假目录/../   文档 🚀.md"), sandbox.path()).unwrap();
    assert_eq!(res, Some(target.canonicalize().unwrap()));
}

#[test]
fn test_challenger_symlink_direct() {
    let sandbox = TestSandbox::new();
    let real_file = sandbox.create_file("source.md", "# Source");
    let symlink = sandbox.path().join("link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_file, &symlink).unwrap();
        let res = resolve_initial_file(Some("link.md"), sandbox.path()).unwrap();
        assert_eq!(res, Some(real_file.canonicalize().unwrap()));
    }
}

#[test]
fn test_challenger_symlink_chain() {
    let sandbox = TestSandbox::new();
    let real_file = sandbox.create_file("source.md", "# Source");
    let link1 = sandbox.path().join("link1.md");
    let link2 = sandbox.path().join("link2.md");
    let link3 = sandbox.path().join("link3.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_file, &link1).unwrap();
        std::os::unix::fs::symlink(&link1, &link2).unwrap();
        std::os::unix::fs::symlink(&link2, &link3).unwrap();
        let res = resolve_initial_file(Some("link3.md"), sandbox.path()).unwrap();
        assert_eq!(res, Some(real_file.canonicalize().unwrap()));
    }
}

#[test]
fn test_challenger_dangling_symlink_fails() {
    let sandbox = TestSandbox::new();
    let dangling = sandbox.path().join("dangling_link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(sandbox.path().join("does_not_exist.md"), &dangling).unwrap();
        let res = resolve_initial_file(Some("dangling_link.md"), sandbox.path());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("File not found: dangling_link.md"));
    }
}

#[test]
fn test_challenger_symlink_to_directory_discovers_readme() {
    let sandbox = TestSandbox::new();
    let dir = sandbox.path().join("content_dir");
    fs::create_dir_all(&dir).unwrap();
    let readme = dir.join("README.md");
    File::create(&readme).unwrap();

    let sym_dir = sandbox.path().join("sym_dir");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&dir, &sym_dir).unwrap();
        let res = resolve_initial_file(Some("sym_dir"), sandbox.path()).unwrap();
        assert_eq!(res, Some(readme.canonicalize().unwrap()));
    }
}

#[test]
fn test_challenger_nonexistent_paths() {
    let sandbox = TestSandbox::new();
    // Nonexistent relative
    let res = resolve_initial_file(Some("missing.md"), sandbox.path());
    assert!(res.is_err());
    // Nonexistent absolute
    let res_abs = resolve_initial_file(Some("/tmp/missing_file_xyz_999.md"), sandbox.path());
    assert!(res_abs.is_err());
    // Nonexistent unicode
    let res_u = resolve_initial_file(Some("不存在.md"), sandbox.path());
    assert!(res_u.is_err());
}

// =========================================================================
// 5. MODE FLAGS PARSING & NORMALIZATION
// =========================================================================

#[test]
fn test_challenger_mode_flag_parsing() {
    for (flag, expected) in [
        ("read", "read"),
        ("split", "split"),
        ("edit", "edit"),
        ("preview", "read"),
        ("editor", "edit"),
        ("READ", "read"),
        ("SPLIT", "split"),
        ("EDIT", "edit"),
        ("PreVieW", "read"),
        ("EdItOr", "edit"),
        ("  read  ", "read"),
        ("unknown", "split"),
        ("12345", "split"),
        ("", "split"),
    ] {
        let args = parse_cli_from(&["typora", "-m", flag]).unwrap();
        assert_eq!(args.normalized_mode(), expected, "Failed for mode flag: {:?}", flag);
    }
}

// =========================================================================
// 6. THEME FLAGS PARSING & NORMALIZATION
// =========================================================================

#[test]
fn test_challenger_theme_flag_parsing() {
    for (flag, expected) in [
        ("github", "github"),
        ("night", "night"),
        ("newsprint", "newsprint"),
        ("gothic", "gothic"),
        ("GITHUB", "github"),
        ("NIGHT", "night"),
        ("NewsPrint", "newsprint"),
        ("GoThIc", "gothic"),
        ("  night  ", "night"),
        ("solarized", "github"),
        ("monokai", "github"),
        ("", "github"),
    ] {
        let args = parse_cli_from(&["typora", "-t", flag]).unwrap();
        assert_eq!(args.normalized_theme(), expected, "Failed for theme flag: {:?}", flag);
    }
}

// =========================================================================
// 7. COMBINATIONS & HELP
// =========================================================================

#[test]
fn test_challenger_help_keyword_variations() {
    let sandbox = TestSandbox::new();
    for kw in ["help", "HELP", "Help", "hElP"] {
        let res = resolve_initial_file(Some(kw), sandbox.path()).unwrap();
        assert!(res.is_some());
        assert!(res.unwrap().exists());
    }
}

#[test]
fn test_challenger_flags_with_positional_interleaving() {
    let args = parse_cli_from(&["typora", "-m", "edit", "my document.md", "-t", "night"]).unwrap();
    assert_eq!(args.file.as_deref(), Some("my document.md"));
    assert_eq!(args.normalized_mode(), "edit");
    assert_eq!(args.normalized_theme(), "night");
}
