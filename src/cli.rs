//! Command-line argument parsing and intelligent file path resolution.
//!
//! Provides CLI parsing via `clap` (derive mode) and path resolution supporting:
//! - Tilde (`~`) expansion
//! - Relative paths resolved against launch cwd
//! - Absolute paths
//! - Case-insensitive "help" keyword
//! - Current directory auto-discovery (README.md, index.md, single *.md)
//! - Untitled scratchpad buffer fallback (returns `Ok(None)`)

use clap::Parser;
use std::path::{Component, Path, PathBuf};

/// Command line arguments for `typora-rust-reader`.
#[derive(Parser, Debug, Clone, PartialEq, Eq, Default)]
#[command(
    name = "typora-rust-reader",
    author = "Typora-RustReader Team",
    version,
    about = "Minimalist distraction-free Markdown reader and editor"
)]
pub struct CliArgs {
    /// Path to markdown file to view, or special keyword "help"
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// Initial UI view mode (read | split | edit)
    #[arg(short = 'm', long = "mode", value_name = "MODE")]
    pub mode: Option<String>,

    /// Initial color theme (github | night | newsprint | gothic)
    #[arg(short = 't', long = "theme", value_name = "THEME")]
    pub theme: Option<String>,
}

impl CliArgs {
    /// Returns the normalized view mode: "read", "split", or "edit".
    /// Defaults to "split" if unspecified or invalid.
    pub fn normalized_mode(&self) -> &str {
        match self
            .mode
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("read") | Some("preview") => "read",
            Some("edit") | Some("editor") => "edit",
            Some("split") => "split",
            _ => "split",
        }
    }

    /// Returns the normalized theme: "github", "night", "newsprint", or "gothic".
    /// Defaults to "github" if unspecified or invalid.
    pub fn normalized_theme(&self) -> &str {
        match self
            .theme
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("github") => "github",
            Some("night") => "night",
            Some("newsprint") => "newsprint",
            Some("gothic") => "gothic",
            _ => "github",
        }
    }
}

/// Parses CLI arguments from `std::env::args()`.
pub fn parse_cli() -> CliArgs {
    CliArgs::parse()
}

/// Parses CLI arguments from an arbitrary iterator (useful for testing).
pub fn parse_cli_from<I, T>(itr: I) -> Result<CliArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    CliArgs::try_parse_from(itr)
}

/// Expands leading `~` or `~/` to user's home directory.
pub fn expand_tilde(path_str: &str) -> PathBuf {
    if path_str == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    } else if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(path_str)
}

/// Normalizes path components without requiring the path to exist on disk.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match components.last() {
                Some(Component::Normal(_)) => {
                    components.pop();
                }
                Some(Component::RootDir) => {}
                _ => components.push(component),
            },
            _ => components.push(component),
        }
    }
    if components.is_empty() {
        if path.as_os_str().is_empty() {
            PathBuf::new()
        } else if path.is_absolute() {
            PathBuf::from("/")
        } else {
            PathBuf::from(".")
        }
    } else {
        components.iter().collect()
    }
}

/// Resolves the initial target file path according to application rules.
pub fn resolve_initial_file(
    file_arg: Option<&str>,
    cwd: &Path,
) -> Result<Option<PathBuf>, String> {
    // 1. Handle missing argument (None -> CWD auto-discovery)
    let arg = match file_arg {
        None => return auto_discover_in_dir(cwd),
        Some(a) => a,
    };

    // 2. Reject explicit empty string argument
    if arg.is_empty() {
        return Err("Empty file path argument provided".to_string());
    }

    // 3. Handle special "help" keyword (case-insensitive)
    if arg.eq_ignore_ascii_case("help") {
        return resolve_help_file(cwd);
    }

    // 4. Resolve explicit path preserving literal whitespace and normalizing relative segments
    let expanded = expand_tilde(arg);
    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    };
    let resolved = normalize_path(&resolved);

    if resolved.exists() {
        if resolved.is_dir() {
            match auto_discover_in_dir(&resolved)? {
                Some(found) => Ok(Some(found)),
                None => Err(format!(
                    "Directory contains no markdown files: {}",
                    resolved.display()
                )),
            }
        } else {
            Ok(Some(resolved.canonicalize().unwrap_or(resolved)))
        }
    } else {
        Err(format!("File not found: {}", arg))
    }
}

/// Auto-discovers README, index, or single markdown file in target directory.
fn auto_discover_in_dir(dir: &Path) -> Result<Option<PathBuf>, String> {
    for cand in &["README.md", "readme.md", "Readme.md", "index.md"] {
        let p = dir.join(cand);
        if p.is_file() {
            return Ok(Some(p.canonicalize().unwrap_or(p)));
        }
    }

    let mut md_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown") {
                        md_files.push(path);
                    }
                }
            }
        }
    }

    md_files.sort();
    if !md_files.is_empty() {
        let first = &md_files[0];
        return Ok(Some(first.canonicalize().unwrap_or_else(|_| first.clone())));
    }

    Ok(None)
}

/// Resolves the help documentation path, extracting embedded copy if needed.
fn resolve_help_file(cwd: &Path) -> Result<Option<PathBuf>, String> {
    let local_help = cwd.join("help").join("README.md");
    if local_help.is_file() {
        return Ok(Some(local_help.canonicalize().unwrap_or(local_help)));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let exe_help = parent.join("help").join("README.md");
            if exe_help.is_file() {
                return Ok(Some(exe_help.canonicalize().unwrap_or(exe_help)));
            }
        }
    }

    let temp_help = std::env::temp_dir().join("typora-rust-reader-help.md");
    std::fs::write(&temp_help, crate::assets::EMBEDDED_HELP_MD)
        .map_err(|e| format!("Failed to write embedded help document: {e}"))?;
    Ok(Some(temp_help))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "typora_cli_test_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_parse_cli_defaults() {
        let args = parse_cli_from(&["typora-rust-reader"]).unwrap();
        assert_eq!(args.file, None);
        assert_eq!(args.mode, None);
        assert_eq!(args.theme, None);
        assert_eq!(args.normalized_mode(), "split");
        assert_eq!(args.normalized_theme(), "github");
    }

    #[test]
    fn test_parse_cli_flags() {
        let args = parse_cli_from(&[
            "typora-rust-reader",
            "doc.md",
            "-m",
            "edit",
            "-t",
            "night",
        ])
        .unwrap();
        assert_eq!(args.file.as_deref(), Some("doc.md"));
        assert_eq!(args.normalized_mode(), "edit");
        assert_eq!(args.normalized_theme(), "night");
    }

    #[test]
    fn test_resolve_help_keyword() {
        let dir = TestDir::new();
        let res = resolve_initial_file(Some("help"), dir.path()).unwrap();
        assert!(res.is_some());
        assert!(res.unwrap().exists());
    }

    #[test]
    fn test_auto_discover_readme() {
        let dir = TestDir::new();
        let readme = dir.path().join("README.md");
        File::create(&readme).unwrap();

        let res = resolve_initial_file(None, dir.path()).unwrap();
        assert_eq!(res, Some(readme.canonicalize().unwrap()));
    }

    #[test]
    fn test_auto_discover_empty_dir() {
        let dir = TestDir::new();
        let res = resolve_initial_file(None, dir.path()).unwrap();
        assert_eq!(res, None); // Untitled scratchpad
    }

    #[test]
    fn test_missing_file_returns_err() {
        let dir = TestDir::new();
        let res = resolve_initial_file(Some("non_existent.md"), dir.path());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("File not found"));
    }

    #[test]
    fn test_resolve_empty_string_returns_err() {
        let dir = TestDir::new();
        let res = resolve_initial_file(Some(""), dir.path());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Empty file path argument provided");
    }

    #[test]
    fn test_resolve_leading_whitespace_path_preserved() {
        let dir = TestDir::new();
        let target = dir.path().join(" spaced.md");
        File::create(&target).unwrap();

        let res = resolve_initial_file(Some(" spaced.md"), dir.path()).unwrap();
        assert_eq!(res, Some(target.canonicalize().unwrap()));
    }

    #[test]
    fn test_resolve_normalized_relative_path_with_nonexistent_segment() {
        let dir = TestDir::new();
        let target = dir.path().join("target.md");
        File::create(&target).unwrap();

        let res = resolve_initial_file(Some("./nonexistent_intermediate/../target.md"), dir.path()).unwrap();
        assert_eq!(res, Some(target.canonicalize().unwrap()));
    }
}
