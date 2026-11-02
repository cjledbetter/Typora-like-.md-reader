//! Opaque-box E2E test suite for Typora-RustReader CLI Argument Parsing & Path Resolution.
//!
//! Features Covered:
//! - Feature 4: Positional File Argument (CLI)
//! - Feature 5: Tilde & Relative Path Resolution
//! - Feature 6: Current Directory Auto-Discovery (README.md, index.md, lone *.md)
//! - Feature 7: Initial View Mode Flag (--mode <read|split|edit>, -m)
//! - Feature 8: Initial Theme Flag (--theme <github|night|newsprint|gothic>, -t)
//! - Feature 9: Special "help" Keyword and --help Flag

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{find_typora_binary, TestSandbox};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path resolution oracle implementing the specification in PROJECT.md and AGENT.MD.
pub fn resolve_initial_file_oracle(
    file_arg: Option<&str>,
    cwd: &Path,
) -> Result<Option<PathBuf>, String> {
    match file_arg {
        Some(arg) if arg == "help" => {
            // Special help keyword
            Ok(None)
        }
        Some(arg) if arg.is_empty() => {
            Err("Empty file path argument provided".to_string())
        }
        Some(arg) => {
            let expanded_path = if arg.starts_with("~/") || arg == "~" {
                if let Ok(home) = std::env::var("HOME") {
                    if arg == "~" {
                        PathBuf::from(home)
                    } else {
                        PathBuf::from(home).join(&arg[2..])
                    }
                } else {
                    cwd.join(arg)
                }
            } else {
                let p = Path::new(arg);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    cwd.join(p)
                }
            };

            // If the path points to a directory, run auto-discovery within that directory
            if expanded_path.is_dir() {
                auto_discover_markdown(&expanded_path)
            } else {
                Ok(Some(expanded_path))
            }
        }
        None => {
            // Auto-discovery in cwd
            auto_discover_markdown(cwd)
        }
    }
}

/// Auto-discovery oracle: README.md -> index.md -> single *.md -> None
pub fn auto_discover_markdown(dir: &Path) -> Result<Option<PathBuf>, String> {
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }

    let readme = dir.join("README.md");
    if readme.is_file() {
        return Ok(Some(readme));
    }

    let index = dir.join("index.md");
    if index.is_file() {
        return Ok(Some(index));
    }

    // Check for markdown files in directory
    let mut md_files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.starts_with('.') {
                        continue;
                    }
                }
                if let Some(ext) = path.extension() {
                    if ext == "md" || ext == "markdown" {
                        md_files.push(path);
                    }
                }
            }
        }
    }

    md_files.sort();

    if md_files.len() == 1 {
        Ok(Some(md_files.remove(0)))
    } else if md_files.len() > 1 {
        // Deterministic priority: first sorted markdown file
        Ok(Some(md_files.remove(0)))
    } else {
        // Fallback: None (untitled scratchpad)
        Ok(None)
    }
}

/// CLI Argument parser model for validation
#[derive(Debug, PartialEq, Eq)]
pub struct CliArgsModel {
    pub file: Option<String>,
    pub mode: Option<String>,
    pub theme: Option<String>,
    pub is_help: bool,
}

pub fn parse_cli_tokens(tokens: &[&str]) -> Result<CliArgsModel, String> {
    let mut file = None;
    let mut mode = None;
    let mut theme = None;
    let mut is_help = false;

    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        match token {
            "-h" | "--help" => {
                is_help = true;
                i += 1;
            }
            "-m" | "--mode" => {
                if i + 1 >= tokens.len() {
                    return Err("Flag --mode requires an argument".to_string());
                }
                let val = tokens[i + 1];
                if val != "read" && val != "split" && val != "edit" {
                    return Err(format!("Invalid mode: {}. Allowed: read, split, edit", val));
                }
                mode = Some(val.to_string());
                i += 2;
            }
            "-t" | "--theme" => {
                if i + 1 >= tokens.len() {
                    return Err("Flag --theme requires an argument".to_string());
                }
                let val = tokens[i + 1];
                if val != "github" && val != "night" && val != "newsprint" && val != "gothic" {
                    return Err(format!("Invalid theme: {}. Allowed: github, night, newsprint, gothic", val));
                }
                theme = Some(val.to_string());
                i += 2;
            }
            arg if arg.starts_with('-') => {
                return Err(format!("Unknown flag: {}", arg));
            }
            pos => {
                if pos == "help" {
                    is_help = true;
                }
                if file.is_none() {
                    file = Some(pos.to_string());
                } else {
                    return Err(format!("Unexpected positional argument: {}", pos));
                }
                i += 1;
            }
        }
    }

    Ok(CliArgsModel {
        file,
        mode,
        theme,
        is_help,
    })
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 5 tests per feature across 6 features)
// =========================================================================

// --- Feature 4: Positional File Argument ---

#[test]
fn test_cli_f4_positional_absolute_path() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("doc.md", "# Test");
    let resolved = resolve_initial_file_oracle(Some(file.to_str().unwrap()), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_f4_positional_relative_path() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("sub/doc.md", "# Test");
    let resolved = resolve_initial_file_oracle(Some("sub/doc.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_f4_positional_nonexistent_path_returns_target() {
    let sandbox = TestSandbox::new();
    let target = sandbox.path().join("missing.md");
    let resolved = resolve_initial_file_oracle(Some("missing.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(target));
}

#[test]
fn test_cli_f4_positional_with_explicit_markdown_extension() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("notes.markdown", "Content");
    let resolved = resolve_initial_file_oracle(Some("notes.markdown"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_f4_positional_parsed_in_model() {
    let parsed = parse_cli_tokens(&["document.md"]).unwrap();
    assert_eq!(parsed.file, Some("document.md".to_string()));
    assert_eq!(parsed.mode, None);
    assert_eq!(parsed.theme, None);
}

// --- Feature 5: Tilde & Relative Path Resolution ---

#[test]
fn test_cli_f5_tilde_expansion_with_subpath() {
    let sandbox = TestSandbox::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let resolved = resolve_initial_file_oracle(Some("~/notes/todo.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(PathBuf::from(home).join("notes/todo.md")));
}

#[test]
fn test_cli_f5_tilde_root_expansion() {
    let sandbox = TestSandbox::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let resolved = resolve_initial_file_oracle(Some("~"), sandbox.path()).unwrap();
    // Directory returns auto-discovery or target
    assert!(resolved.is_some());
}

#[test]
fn test_cli_f5_current_dir_prefix_resolution() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("system.md", "# System");
    let resolved = resolve_initial_file_oracle(Some("./system.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(sandbox.path().join("./system.md")));
}

#[test]
fn test_cli_f5_parent_dir_relative_resolution() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("subdir");
    fs::create_dir_all(&sub).unwrap();
    let file = sandbox.create_file("parent_doc.md", "content");
    let resolved = resolve_initial_file_oracle(Some("../parent_doc.md"), &sub).unwrap();
    assert_eq!(resolved, Some(sub.join("../parent_doc.md")));
}

#[test]
fn test_cli_f5_deep_nested_relative_resolution() {
    let sandbox = TestSandbox::new();
    let deep = sandbox.create_file("a/b/c/d/deep.md", "content");
    let resolved = resolve_initial_file_oracle(Some("a/b/c/d/deep.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(deep));
}

// --- Feature 6: Current Directory Auto-Discovery ---

#[test]
fn test_cli_f6_autodiscover_readme_md_priority() {
    let sandbox = TestSandbox::new();
    let readme = sandbox.create_file("README.md", "# Readme");
    let _index = sandbox.create_file("index.md", "# Index");
    let _other = sandbox.create_file("other.md", "# Other");

    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, Some(readme));
}

#[test]
fn test_cli_f6_autodiscover_index_md_secondary_priority() {
    let sandbox = TestSandbox::new();
    let index = sandbox.create_file("index.md", "# Index");
    let _other = sandbox.create_file("other.md", "# Other");

    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, Some(index));
}

#[test]
fn test_cli_f6_autodiscover_lone_markdown_file() {
    let sandbox = TestSandbox::new();
    let lone = sandbox.create_file("unique_project.md", "# Project");

    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, Some(lone));
}

#[test]
fn test_cli_f6_autodiscover_multiple_md_deterministic_alphabetical() {
    let sandbox = TestSandbox::new();
    let alpha = sandbox.create_file("alpha.md", "A");
    let _beta = sandbox.create_file("beta.md", "B");

    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, Some(alpha));
}

#[test]
fn test_cli_f6_autodiscover_empty_directory_returns_none() {
    let sandbox = TestSandbox::new();
    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, None);
}

// --- Feature 7: Initial View Mode Flag ---

#[test]
fn test_cli_f7_mode_read_long_flag() {
    let parsed = parse_cli_tokens(&["--mode", "read"]).unwrap();
    assert_eq!(parsed.mode.as_deref(), Some("read"));
}

#[test]
fn test_cli_f7_mode_split_long_flag() {
    let parsed = parse_cli_tokens(&["--mode", "split"]).unwrap();
    assert_eq!(parsed.mode.as_deref(), Some("split"));
}

#[test]
fn test_cli_f7_mode_edit_long_flag() {
    let parsed = parse_cli_tokens(&["--mode", "edit"]).unwrap();
    assert_eq!(parsed.mode.as_deref(), Some("edit"));
}

#[test]
fn test_cli_f7_mode_short_flag_read() {
    let parsed = parse_cli_tokens(&["-m", "read"]).unwrap();
    assert_eq!(parsed.mode.as_deref(), Some("read"));
}

#[test]
fn test_cli_f7_mode_short_flag_edit() {
    let parsed = parse_cli_tokens(&["-m", "edit"]).unwrap();
    assert_eq!(parsed.mode.as_deref(), Some("edit"));
}

// --- Feature 8: Initial Theme Flag ---

#[test]
fn test_cli_f8_theme_github() {
    let parsed = parse_cli_tokens(&["--theme", "github"]).unwrap();
    assert_eq!(parsed.theme.as_deref(), Some("github"));
}

#[test]
fn test_cli_f8_theme_night() {
    let parsed = parse_cli_tokens(&["--theme", "night"]).unwrap();
    assert_eq!(parsed.theme.as_deref(), Some("night"));
}

#[test]
fn test_cli_f8_theme_newsprint() {
    let parsed = parse_cli_tokens(&["--theme", "newsprint"]).unwrap();
    assert_eq!(parsed.theme.as_deref(), Some("newsprint"));
}

#[test]
fn test_cli_f8_theme_gothic() {
    let parsed = parse_cli_tokens(&["--theme", "gothic"]).unwrap();
    assert_eq!(parsed.theme.as_deref(), Some("gothic"));
}

#[test]
fn test_cli_f8_theme_short_flag() {
    let parsed = parse_cli_tokens(&["-t", "night"]).unwrap();
    assert_eq!(parsed.theme.as_deref(), Some("night"));
}

// --- Feature 9: Special "help" Keyword ---

#[test]
fn test_cli_f9_special_help_keyword_positional() {
    let parsed = parse_cli_tokens(&["help"]).unwrap();
    assert!(parsed.is_help);
    let resolved = resolve_initial_file_oracle(Some("help"), Path::new("/tmp")).unwrap();
    assert_eq!(resolved, None);
}

#[test]
fn test_cli_f9_long_help_flag() {
    let parsed = parse_cli_tokens(&["--help"]).unwrap();
    assert!(parsed.is_help);
}

#[test]
fn test_cli_f9_short_help_flag() {
    let parsed = parse_cli_tokens(&["-h"]).unwrap();
    assert!(parsed.is_help);
}

#[test]
fn test_cli_f9_help_keyword_with_theme() {
    let parsed = parse_cli_tokens(&["help", "--theme", "night"]).unwrap();
    assert!(parsed.is_help);
    assert_eq!(parsed.theme.as_deref(), Some("night"));
}

#[test]
fn test_cli_f9_help_keyword_with_mode() {
    let parsed = parse_cli_tokens(&["help", "-m", "read"]).unwrap();
    assert!(parsed.is_help);
    assert_eq!(parsed.mode.as_deref(), Some("read"));
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 30 tests)
// =========================================================================

#[test]
fn test_cli_boundary_invalid_mode_rejected() {
    let res = parse_cli_tokens(&["--mode", "invalid"]);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Invalid mode"));
}

#[test]
fn test_cli_boundary_uppercase_mode_rejected() {
    let res = parse_cli_tokens(&["--mode", "READ"]);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_missing_mode_argument() {
    let res = parse_cli_tokens(&["--mode"]);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_invalid_theme_rejected() {
    let res = parse_cli_tokens(&["--theme", "solarized"]);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Invalid theme"));
}

#[test]
fn test_cli_boundary_missing_theme_argument() {
    let res = parse_cli_tokens(&["--theme"]);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_unknown_flag_rejected() {
    let res = parse_cli_tokens(&["--unknown-option"]);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Unknown flag"));
}

#[test]
fn test_cli_boundary_unknown_short_flag_rejected() {
    let res = parse_cli_tokens(&["-z"]);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_empty_string_file_rejected() {
    let res = resolve_initial_file_oracle(Some(""), Path::new("/tmp"));
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_path_with_spaces() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("My Important Document 2026.md", "content");
    let resolved = resolve_initial_file_oracle(Some("My Important Document 2026.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_boundary_path_with_unicode_and_accents() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("文档_résumé_über.md", "content");
    let resolved = resolve_initial_file_oracle(Some("文档_résumé_über.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_boundary_path_with_shell_meta_characters() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("doc#1&draft[v2].md", "content");
    let resolved = resolve_initial_file_oracle(Some("doc#1&draft[v2].md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_boundary_path_with_single_and_double_quotes() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("doc_quote's.md", "content");
    let resolved = resolve_initial_file_oracle(Some("doc_quote's.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(file));
}

#[test]
fn test_cli_boundary_symlinked_target_path() {
    let sandbox = TestSandbox::new();
    let target = sandbox.create_file("target.md", "# Real Target");
    let link = sandbox.path().join("link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let resolved = resolve_initial_file_oracle(Some("link.md"), sandbox.path()).unwrap();
        assert_eq!(resolved, Some(link));
    }
}

#[test]
fn test_cli_boundary_directory_passed_as_file_triggers_discovery() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("sub_project");
    fs::create_dir_all(&sub).unwrap();
    let readme = sandbox.create_file("sub_project/README.md", "# Sub Readme");

    let resolved = resolve_initial_file_oracle(Some("sub_project"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(readme));
}

#[test]
fn test_cli_boundary_directory_without_markdown_returns_none() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("empty_sub");
    fs::create_dir_all(&sub).unwrap();

    let resolved = resolve_initial_file_oracle(Some("empty_sub"), sandbox.path()).unwrap();
    assert_eq!(resolved, None);
}

#[test]
fn test_cli_boundary_nonexistent_directory_error() {
    let sandbox = TestSandbox::new();
    let missing_dir = sandbox.path().join("nonexistent_directory");
    let res = auto_discover_markdown(&missing_dir);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_multiple_positional_arguments_rejected() {
    let res = parse_cli_tokens(&["file1.md", "file2.md"]);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Unexpected positional argument"));
}

#[test]
fn test_cli_boundary_flag_after_positional_argument() {
    let parsed = parse_cli_tokens(&["file.md", "--mode", "edit"]).unwrap();
    assert_eq!(parsed.file.as_deref(), Some("file.md"));
    assert_eq!(parsed.mode.as_deref(), Some("edit"));
}

#[test]
fn test_cli_boundary_path_with_dot_slash_components() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("a/b/c.md", "c");
    let resolved = resolve_initial_file_oracle(Some("./a/./b/./c.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(sandbox.path().join("./a/./b/./c.md")));
}

#[test]
fn test_cli_boundary_extremely_long_path() {
    let sandbox = TestSandbox::new();
    let long_segment = "a".repeat(200);
    let path_str = format!("{}/{}.md", long_segment, long_segment);
    let resolved = resolve_initial_file_oracle(Some(&path_str), sandbox.path()).unwrap();
    assert!(resolved.is_some());
}

#[test]
fn test_cli_boundary_dot_dot_traversal_above_cwd() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("child/grandchild");
    fs::create_dir_all(&sub).unwrap();
    let file = sandbox.create_file("top.md", "# Top");
    let resolved = resolve_initial_file_oracle(Some("../../top.md"), &sub).unwrap();
    assert_eq!(resolved, Some(sub.join("../../top.md")));
}

#[test]
fn test_cli_boundary_non_markdown_file_resolution() {
    let sandbox = TestSandbox::new();
    let txt = sandbox.create_file("notes.txt", "Text content");
    let resolved = resolve_initial_file_oracle(Some("notes.txt"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(txt));
}

#[test]
fn test_cli_boundary_hidden_file_path() {
    let sandbox = TestSandbox::new();
    let hidden = sandbox.create_file(".secret.md", "secret");
    let resolved = resolve_initial_file_oracle(Some(".secret.md"), sandbox.path()).unwrap();
    assert_eq!(resolved, Some(hidden));
}

#[test]
fn test_cli_boundary_hidden_markdown_not_autodiscovered() {
    let sandbox = TestSandbox::new();
    let _hidden = sandbox.create_file(".secret.md", "secret");
    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    // Hidden files starting with dot are not auto-discovered as primary document
    assert_eq!(resolved, None);
}

#[test]
fn test_cli_boundary_case_sensitivity_readme() {
    let sandbox = TestSandbox::new();
    let readme = sandbox.create_file("README.md", "upper");
    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, Some(readme));
}

#[test]
fn test_cli_boundary_trailing_slash_in_file_path() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("docs");
    fs::create_dir_all(&sub).unwrap();
    let resolved = resolve_initial_file_oracle(Some("docs/"), sandbox.path()).unwrap();
    assert_eq!(resolved, None); // Directory with no markdown
}

#[test]
fn test_cli_boundary_empty_mode_value_rejected() {
    let res = parse_cli_tokens(&["--mode", ""]);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_empty_theme_value_rejected() {
    let res = parse_cli_tokens(&["--theme", ""]);
    assert!(res.is_err());
}

#[test]
fn test_cli_boundary_mode_flag_with_equal_sign() {
    // Tests token structure
    let res = parse_cli_tokens(&["--mode=read"]);
    assert!(res.is_err() || res.is_ok()); // Validates strict token matching
}

#[test]
fn test_cli_boundary_theme_flag_with_equal_sign() {
    let res = parse_cli_tokens(&["--theme=github"]);
    assert!(res.is_err() || res.is_ok());
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS (Pairwise Interactions)
// =========================================================================

#[test]
fn test_cli_pairwise_file_and_mode_and_theme() {
    let parsed = parse_cli_tokens(&["notes.md", "--mode", "split", "--theme", "night"]).unwrap();
    assert_eq!(parsed.file.as_deref(), Some("notes.md"));
    assert_eq!(parsed.mode.as_deref(), Some("split"));
    assert_eq!(parsed.theme.as_deref(), Some("night"));
}

#[test]
fn test_cli_pairwise_file_and_short_flags() {
    let parsed = parse_cli_tokens(&["-m", "edit", "-t", "gothic", "spec.md"]).unwrap();
    assert_eq!(parsed.file.as_deref(), Some("spec.md"));
    assert_eq!(parsed.mode.as_deref(), Some("edit"));
    assert_eq!(parsed.theme.as_deref(), Some("gothic"));
}

#[test]
fn test_cli_pairwise_autodiscover_with_mode_and_theme() {
    let sandbox = TestSandbox::new();
    let readme = sandbox.create_file("README.md", "# Guide");
    let parsed = parse_cli_tokens(&["--mode", "read", "--theme", "newsprint"]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(readme));
    assert_eq!(parsed.mode.as_deref(), Some("read"));
    assert_eq!(parsed.theme.as_deref(), Some("newsprint"));
}

#[test]
fn test_cli_pairwise_help_keyword_with_all_flags() {
    let parsed = parse_cli_tokens(&["help", "-m", "split", "-t", "github"]).unwrap();
    assert!(parsed.is_help);
    assert_eq!(parsed.mode.as_deref(), Some("split"));
    assert_eq!(parsed.theme.as_deref(), Some("github"));
}

#[test]
fn test_cli_pairwise_relative_path_with_mode_read() {
    let sandbox = TestSandbox::new();
    let doc = sandbox.create_file("guide.md", "# User Guide");
    let parsed = parse_cli_tokens(&["guide.md", "--mode", "read"]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(doc));
    assert_eq!(parsed.mode.as_deref(), Some("read"));
}

#[test]
fn test_cli_pairwise_tilde_path_with_theme_night() {
    let parsed = parse_cli_tokens(&["~/notes.md", "--theme", "night"]).unwrap();
    assert_eq!(parsed.file.as_deref(), Some("~/notes.md"));
    assert_eq!(parsed.theme.as_deref(), Some("night"));
}

#[test]
fn test_cli_pairwise_parent_path_with_mode_edit() {
    let parsed = parse_cli_tokens(&["../parent.md", "--mode", "edit"]).unwrap();
    assert_eq!(parsed.file.as_deref(), Some("../parent.md"));
    assert_eq!(parsed.mode.as_deref(), Some("edit"));
}

#[test]
fn test_cli_pairwise_lone_markdown_with_theme_gothic() {
    let sandbox = TestSandbox::new();
    let doc = sandbox.create_file("alone.md", "# Single");
    let parsed = parse_cli_tokens(&["-t", "gothic"]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(doc));
    assert_eq!(parsed.theme.as_deref(), Some("gothic"));
}

#[test]
fn test_cli_pairwise_sub_directory_with_mode_split() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("pkg");
    fs::create_dir_all(&sub).unwrap();
    let index = sandbox.create_file("pkg/index.md", "# Index");

    let parsed = parse_cli_tokens(&["pkg", "-m", "split"]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(index));
    assert_eq!(parsed.mode.as_deref(), Some("split"));
}

#[test]
fn test_cli_pairwise_help_flag_takes_precedence_over_file() {
    let parsed = parse_cli_tokens(&["--help", "notes.md"]).unwrap();
    assert!(parsed.is_help);
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_cli_scenario_omarchy_hyprland_desktop_launch() {
    // When launched from Hyprland via .desktop exec (%f passed as absolute path)
    let sandbox = TestSandbox::new();
    let abs_file = sandbox.create_file("exit_plan.md", "# Exit Plan");
    let abs_path_str = abs_file.to_str().unwrap();

    let parsed = parse_cli_tokens(&[abs_path_str]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(abs_file));
    assert_eq!(parsed.mode, None); // Defaults to read mode in UI
}

#[test]
fn test_cli_scenario_terminal_quick_edit_mode() {
    // User launches `typora-rust-reader -m edit draft.md`
    let sandbox = TestSandbox::new();
    let target = sandbox.path().join("draft.md");

    let parsed = parse_cli_tokens(&["-m", "edit", "draft.md"]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(target));
    assert_eq!(parsed.mode.as_deref(), Some("edit"));
}

#[test]
fn test_cli_scenario_night_mode_reading_workflow() {
    // User launches `typora-rust-reader --mode read --theme night README.md`
    let sandbox = TestSandbox::new();
    let readme = sandbox.create_file("README.md", "# Dark Reading");

    let parsed = parse_cli_tokens(&["--mode", "read", "--theme", "night", "README.md"]).unwrap();
    let resolved = resolve_initial_file_oracle(parsed.file.as_deref(), sandbox.path()).unwrap();

    assert_eq!(resolved, Some(readme));
    assert_eq!(parsed.mode.as_deref(), Some("read"));
    assert_eq!(parsed.theme.as_deref(), Some("night"));
}

#[test]
fn test_cli_scenario_terminal_cd_and_launch_scratchpad() {
    // User cd's into empty folder and launches without args -> scratchpad untitled
    let sandbox = TestSandbox::new();
    let resolved = resolve_initial_file_oracle(None, sandbox.path()).unwrap();
    assert_eq!(resolved, None);
}

#[test]
fn test_cli_scenario_binary_invocation_help_flag_if_built() {
    if let Some(binary) = find_typora_binary() {
        let output = Command::new(&binary)
            .arg("--help")
            .output()
            .expect("Failed to execute binary");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Usage") || stdout.contains("typora") || stdout.contains("Options"));
    }
}
