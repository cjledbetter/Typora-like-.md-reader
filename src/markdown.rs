//! Typora-RustReader Markdown AST, Table of Contents (Outline) & Statistics Engine.
//!
//! Provides high-performance Markdown parsing via `pulldown-cmark = "0.13"`:
//! - Heading Outline / TOC Extraction with 1-indexed line numbers.
//! - Marked.js-compatible slugification and anchor disambiguation.
//! - Document Statistics (words, characters, characters excluding whitespace, lines, reading time).

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Table of Contents outline item adhering to the Typora-RustReader contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocItem {
    /// Heading level (1 to 6).
    pub level: u32,
    /// Clean heading title text with inline formatting and HTML stripped.
    #[serde(alias = "text")]
    pub title: String,
    /// URL/DOM anchor slug matching marked.js rules.
    #[serde(alias = "slug")]
    pub anchor: String,
    /// 1-indexed source line number in the Markdown document.
    #[serde(default)]
    pub line: usize,
}

/// Outline heading item adhering to the Milestone 3 specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingItem {
    /// Heading level (1 to 6).
    pub level: u32,
    /// Clean heading text.
    #[serde(alias = "title")]
    pub text: String,
    /// Anchor slug matching marked.js rules.
    #[serde(alias = "anchor")]
    pub slug: String,
    /// Optional source line number (defaults to 0 if omitted).
    #[serde(default)]
    pub line: usize,
}

impl HeadingItem {
    /// Creates a new HeadingItem without line number.
    pub fn new(level: u32, text: impl Into<String>, slug: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
            slug: slug.into(),
            line: 0,
        }
    }
}

impl From<TocItem> for HeadingItem {
    fn from(item: TocItem) -> Self {
        Self {
            level: item.level,
            text: item.title,
            slug: item.anchor,
            line: item.line,
        }
    }
}

impl From<HeadingItem> for TocItem {
    fn from(item: HeadingItem) -> Self {
        Self {
            level: item.level,
            title: item.text,
            anchor: item.slug,
            line: item.line,
        }
    }
}

/// Compact document statistics adhering to the Typora-RustReader contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocStats {
    /// Word count excluding markdown delimiters.
    pub words: usize,
    /// Total character count including spaces and newlines.
    #[serde(alias = "characters")]
    pub chars: usize,
    /// Line count (0 for empty document).
    pub lines: usize,
    /// Estimated reading time in minutes at 200 words per minute (minimum 1 if words > 0).
    pub reading_time_minutes: usize,
}

impl DocStats {
    /// Returns zeroed empty document statistics.
    pub fn empty() -> Self {
        Self {
            words: 0,
            chars: 0,
            lines: 0,
            reading_time_minutes: 0,
        }
    }
}

impl Default for DocStats {
    fn default() -> Self {
        Self::empty()
    }
}

/// Comprehensive document statistics adhering to the Milestone 3 specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentStats {
    /// Word count excluding markdown delimiters.
    pub words: usize,
    /// Total character count including whitespace.
    #[serde(alias = "chars")]
    pub characters: usize,
    /// Character count excluding whitespace.
    pub characters_no_spaces: usize,
    /// Total line count (0 for empty document).
    pub lines: usize,
    /// Estimated reading time in minutes at 200 words per minute (minimum 1 if words > 0).
    pub reading_time_minutes: usize,
}

impl DocumentStats {
    /// Returns zeroed empty document statistics.
    pub fn empty() -> Self {
        Self {
            words: 0,
            characters: 0,
            characters_no_spaces: 0,
            lines: 0,
            reading_time_minutes: 0,
        }
    }
}

impl Default for DocumentStats {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<DocumentStats> for DocStats {
    fn from(stats: DocumentStats) -> Self {
        Self {
            words: stats.words,
            chars: stats.characters,
            lines: stats.lines,
            reading_time_minutes: stats.reading_time_minutes,
        }
    }
}

impl From<DocStats> for DocumentStats {
    fn from(stats: DocStats) -> Self {
        Self {
            words: stats.words,
            characters: stats.chars,
            characters_no_spaces: stats.chars,
            lines: stats.lines,
            reading_time_minutes: stats.reading_time_minutes,
        }
    }
}

/// Converts `pulldown_cmark::HeadingLevel` into integer 1..=6.
#[inline]
pub fn heading_level_to_u32(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Marked.js-compatible slug generation with duplicate anchor disambiguation (`-1`, `-2`, etc.).
///
/// Rules:
/// 1. Lowercase all characters.
/// 2. Keep unicode alphanumeric characters (`c.is_alphanumeric()`).
/// 3. Replace spaces, hyphens, and underscores with a single hyphen `-`.
/// 4. Discard all punctuation and symbols (without inserting hyphens).
/// 5. Strip leading and trailing hyphens.
/// 6. Fallback to `"section"` if base slug is empty.
/// 7. Suffix duplicate slugs with `-1`, `-2`, `-3`, etc.
pub fn marked_slugify(title: &str, existing: &mut HashSet<String>) -> String {
    let mut base = String::new();
    let mut prev_hyphen = false;

    for c in title.chars() {
        if c.is_alphanumeric() {
            base.extend(c.to_lowercase());
            prev_hyphen = false;
        } else if (c == ' ' || c == '-' || c == '_') && !prev_hyphen && !base.is_empty() {
            base.push('-');
            prev_hyphen = true;
        }
    }

    if base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base.push_str("section");
    }

    let mut slug = base.clone();
    let mut counter = 1;
    while existing.contains(&slug) {
        slug = format!("{base}-{counter}");
        counter += 1;
    }
    existing.insert(slug.clone());
    slug
}

/// Marked.js-compatible slug generation alias matching Milestone 3 specification.
#[inline]
pub fn slugify_marked(title: &str, existing: &mut HashSet<String>) -> String {
    marked_slugify(title, existing)
}

/// Convenience slugifier for a single heading title (no collision tracking).
pub fn slugify(title: &str) -> String {
    let mut existing = HashSet::new();
    marked_slugify(title, &mut existing)
}

/// Strips markdown inline markup (bold, italic, links, code spans, HTML tags) from text.
pub fn strip_inline_markdown(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '~' | '`' => {
                // Skip formatting delimiter
            }
            '[' => {
                // Link text: keep text inside [...], skip (url)
                let mut link_text = String::new();
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    link_text.push(inner);
                }
                // Skip URL part if present
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for inner in chars.by_ref() {
                        if inner == ')' {
                            break;
                        }
                    }
                }
                result.push_str(&strip_inline_markdown(&link_text));
            }
            '<' => {
                // HTML tag: skip until >
                for inner in chars.by_ref() {
                    if inner == '>' {
                        break;
                    }
                }
            }
            other => result.push(other),
        }
    }

    result.trim().to_string()
}

/// Extracts Table of Contents (`Vec<TocItem>`) using pulldown-cmark AST parser.
pub fn extract_toc(markdown: &str) -> Vec<TocItem> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(markdown, options).into_offset_iter();
    let mut toc = Vec::new();
    let mut existing_anchors = HashSet::new();

    let mut current_heading: Option<(HeadingLevel, usize, String)> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                // Calculate 1-indexed line number from byte offset
                let line = 1 + markdown[..range.start].bytes().filter(|&b| b == b'\n').count();
                current_heading = Some((level, line, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, line, text)) = current_heading.take() {
                    let clean_title = text.trim().to_string();
                    let anchor = marked_slugify(&clean_title, &mut existing_anchors);
                    toc.push(TocItem {
                        level: heading_level_to_u32(level),
                        title: clean_title,
                        anchor,
                        line,
                    });
                }
            }
            Event::Text(t) => {
                if let Some((_, _, ref mut text)) = current_heading {
                    text.push_str(&t);
                }
            }
            Event::Code(c) => {
                if let Some((_, _, ref mut text)) = current_heading {
                    text.push_str(&c);
                }
            }
            Event::InlineMath(m) => {
                if let Some((_, _, ref mut text)) = current_heading {
                    text.push('$');
                    text.push_str(&m);
                    text.push('$');
                }
            }
            Event::DisplayMath(m) => {
                if let Some((_, _, ref mut text)) = current_heading {
                    text.push_str("$$");
                    text.push_str(&m);
                    text.push_str("$$");
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, ref mut text)) = current_heading {
                    text.push(' ');
                }
            }
            _ => {}
        }
    }

    toc
}

/// Extracts heading outline items (`Vec<HeadingItem>`).
pub fn extract_headings(markdown: &str) -> Vec<HeadingItem> {
    extract_toc(markdown).into_iter().map(Into::into).collect()
}

/// Computes comprehensive DocumentStats (`words`, `characters`, `characters_no_spaces`, `lines`, `reading_time_minutes`).
pub fn extract_document_stats(markdown: &str) -> DocumentStats {
    let lines = if markdown.is_empty() {
        0
    } else {
        markdown.lines().count()
    };
    let characters = markdown.chars().count();
    let characters_no_spaces = markdown.chars().filter(|c| !c.is_whitespace()).count();
    let words = markdown
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .filter(|w| !w.chars().all(|c| c == '#' || c == '=' || c == '-' || c == '|' || c == '`' || c == '*'))
        .count();
    let reading_time_minutes = if words == 0 {
        0
    } else {
        ((words as f64) / 200.0).ceil() as usize
    };

    DocumentStats {
        words,
        characters,
        characters_no_spaces,
        lines,
        reading_time_minutes,
    }
}

/// Computes compact DocStats (`words`, `chars`, `lines`, `reading_time_minutes`).
pub fn calculate_doc_stats(markdown: &str) -> DocStats {
    extract_document_stats(markdown).into()
}

/// Computes document statistics adhering to the Milestone 3 specification.
#[inline]
pub fn compute_stats(markdown: &str) -> DocumentStats {
    extract_document_stats(markdown)
}

/// Canonical combined extractor returning both outline TOC items and document statistics.
pub fn extract_toc_and_stats(markdown: &str) -> (Vec<TocItem>, DocStats) {
    let toc = extract_toc(markdown);
    let stats = calculate_doc_stats(markdown);
    (toc, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extracts_h1_through_h6() {
        let md = "# Level 1\n## Level 2\n### Level 3\n#### Level 4\n##### Level 5\n###### Level 6";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 6);
        for (i, item) in toc.iter().enumerate() {
            assert_eq!(item.level, (i + 1) as u32);
            assert_eq!(item.title, format!("Level {}", i + 1));
            assert_eq!(item.anchor, format!("level-{}", i + 1));
            assert_eq!(item.line, i + 1);
        }
    }

    #[test]
    fn test_anchor_slugification_simple() {
        let md = "# Introduction to Typora";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].anchor, "introduction-to-typora");
    }

    #[test]
    fn test_duplicate_heading_disambiguation() {
        let md = "# Overview\n# Overview\n# Overview";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 3);
        assert_eq!(toc[0].anchor, "overview");
        assert_eq!(toc[1].anchor, "overview-1");
        assert_eq!(toc[2].anchor, "overview-2");
    }

    #[test]
    fn test_code_block_headings_ignored() {
        let md = "# Real Heading\n```rust\n# Fake Heading in Code\n```\n## Second Real Heading";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].title, "Real Heading");
        assert_eq!(toc[1].title, "Second Real Heading");
    }

    #[test]
    fn test_inline_formatting_stripped_from_title() {
        let md = "# **Bold Heading** and *Italic* with `code`";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "Bold Heading and Italic with code");
        assert_eq!(toc[0].anchor, "bold-heading-and-italic-with-code");
    }

    #[test]
    fn test_setext_heading_level_1_and_2() {
        let md = "First Section\n=============\n\nSecond Section\n--------------";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].level, 1);
        assert_eq!(toc[0].title, "First Section");
        assert_eq!(toc[0].anchor, "first-section");

        assert_eq!(toc[1].level, 2);
        assert_eq!(toc[1].title, "Second Section");
        assert_eq!(toc[1].anchor, "second-section");
    }

    #[test]
    fn test_word_count_accurate() {
        let md = "One two three four five six seven eight nine ten.";
        let (_, stats) = extract_toc_and_stats(md);
        assert_eq!(stats.words, 10);
    }

    #[test]
    fn test_character_count_accurate() {
        let md = "Hello World!";
        let stats = extract_document_stats(md);
        assert_eq!(stats.characters, 12);
        assert_eq!(stats.characters_no_spaces, 11);
    }

    #[test]
    fn test_line_count_accurate() {
        let md = "Line 1\nLine 2\nLine 3\nLine 4";
        let (_, stats) = extract_toc_and_stats(md);
        assert_eq!(stats.lines, 4);
    }

    #[test]
    fn test_reading_time_estimation() {
        let words_200 = "word ".repeat(200);
        let (_, stats_200) = extract_toc_and_stats(&words_200);
        assert_eq!(stats_200.reading_time_minutes, 1);

        let words_201 = "word ".repeat(201);
        let (_, stats_201) = extract_toc_and_stats(&words_201);
        assert_eq!(stats_201.reading_time_minutes, 2);

        let words_500 = "word ".repeat(500);
        let (_, stats_500) = extract_toc_and_stats(&words_500);
        assert_eq!(stats_500.reading_time_minutes, 3);
    }

    #[test]
    fn test_boundary_empty_document() {
        let (toc, stats) = extract_toc_and_stats("");
        assert!(toc.is_empty());
        assert_eq!(stats.words, 0);
        assert_eq!(stats.chars, 0);
        assert_eq!(stats.lines, 0);
        assert_eq!(stats.reading_time_minutes, 0);
    }

    #[test]
    fn test_boundary_only_whitespace_and_newlines() {
        let (toc, stats) = extract_toc_and_stats("   \n\t\n   \n");
        assert!(toc.is_empty());
        assert_eq!(stats.words, 0);
        assert_eq!(stats.reading_time_minutes, 0);
    }

    #[test]
    fn test_boundary_cjk_unicode_heading() {
        let md = "# 快速入门 指南";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "快速入门 指南");
        assert_eq!(toc[0].anchor, "快速入门-指南");
    }

    #[test]
    fn test_boundary_punctuation_and_symbols_in_heading() {
        let md = "# C++ & Rust: The 2026 Edition (v2.0)!";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].anchor, "c-rust-the-2026-edition-v20");
    }

    #[test]
    fn test_boundary_hyperlink_in_heading() {
        let md = "# Visit [GitHub](https://github.com) Today";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "Visit GitHub Today");
        assert_eq!(toc[0].anchor, "visit-github-today");
    }

    #[test]
    fn test_boundary_html_tag_in_heading() {
        let md = "# <span style=\"color:red\">Alert</span> Notice";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "Alert Notice");
        assert_eq!(toc[0].anchor, "alert-notice");
    }

    #[test]
    fn test_boundary_heading_without_alphanumerics_fallback() {
        let md = "# ???";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].anchor, "section");
    }

    #[test]
    fn test_boundary_skipped_levels() {
        let md = "# Top Level 1\n##### Skipped Level 5";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].level, 1);
        assert_eq!(toc[1].level, 5);
    }

    #[test]
    fn test_boundary_consecutive_identical_duplicates() {
        let mut md = String::new();
        for _ in 0..10 {
            md.push_str("# Section\n");
        }
        let (toc, _) = extract_toc_and_stats(&md);

        assert_eq!(toc.len(), 10);
        assert_eq!(toc[0].anchor, "section");
        for i in 1..10 {
            assert_eq!(toc[i].anchor, format!("section-{i}"));
        }
    }

    #[test]
    fn test_boundary_tilde_fenced_code_blocks() {
        let md = "~~~python\n# Not a heading\ndef foo(): pass\n~~~\n# Real Title";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "Real Title");
    }

    #[test]
    fn test_boundary_line_numbers_with_blank_lines() {
        let md = "\n\n\n# Line 4 Heading\n\n\n\n## Line 8 Heading";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].line, 4);
        assert_eq!(toc[1].line, 8);
    }

    #[test]
    fn test_boundary_gfm_tables_words_counted() {
        let md = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |";
        let (_, stats) = extract_toc_and_stats(md);
        assert!(stats.words >= 6);
        assert_eq!(stats.lines, 4);
    }

    #[test]
    fn test_boundary_math_block_katex_counted() {
        let md = "# Math\n$$\n\\int_{-\\infty}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}\n$$\nText after";
        let (toc, stats) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "Math");
        assert!(stats.words > 0);
    }

    #[test]
    fn test_boundary_inline_katex_in_heading() {
        let md = "# The $E=mc^2$ Equation";
        let (toc, _) = extract_toc_and_stats(md);

        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "The $E=mc^2$ Equation");
    }

    #[test]
    fn test_boundary_crlf_line_counting() {
        let md_lf = "Line 1\nLine 2\nLine 3";
        let md_crlf = "Line 1\r\nLine 2\r\nLine 3";

        let (_, stats_lf) = extract_toc_and_stats(md_lf);
        let (_, stats_crlf) = extract_toc_and_stats(md_crlf);

        assert_eq!(stats_lf.lines, stats_crlf.lines);
        assert_eq!(stats_lf.words, stats_crlf.words);
    }

    #[test]
    fn test_boundary_hash_without_space_is_not_heading() {
        let md = "#not_a_heading and #another_tag";
        let (toc, _) = extract_toc_and_stats(md);
        assert!(toc.is_empty());
    }

    #[test]
    fn test_boundary_more_than_six_hashes_is_not_heading() {
        let md = "####### Seven Hashes";
        let (toc, _) = extract_toc_and_stats(md);
        assert!(toc.is_empty());
    }

    #[test]
    fn test_boundary_single_word_document() {
        let (toc, stats) = extract_toc_and_stats("Word");
        assert!(toc.is_empty());
        assert_eq!(stats.words, 1);
        assert_eq!(stats.reading_time_minutes, 1);
    }

    #[test]
    fn test_boundary_reading_time_ceiling() {
        let (_, s1) = extract_toc_and_stats("one");
        assert_eq!(s1.reading_time_minutes, 1);

        let text_400 = "word ".repeat(400);
        let (_, s400) = extract_toc_and_stats(&text_400);
        assert_eq!(s400.reading_time_minutes, 2);

        let text_401 = "word ".repeat(401);
        let (_, s401) = extract_toc_and_stats(&text_401);
        assert_eq!(s401.reading_time_minutes, 3);
    }

    #[test]
    fn test_headings_conversion_and_serde() {
        let md = "# Title 1\n## Title 2";
        let headings = extract_headings(md);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Title 1");
        assert_eq!(headings[0].slug, "title-1");

        // Test JSON serialization & deserialization
        let json = serde_json::to_string(&headings).unwrap();
        let deserialized: Vec<HeadingItem> = serde_json::from_str(&json).unwrap();
        assert_eq!(headings, deserialized);

        // Deserializing TocItem from HeadingItem JSON (field aliases)
        let toc_items: Vec<TocItem> = serde_json::from_str(&json).unwrap();
        assert_eq!(toc_items[0].title, "Title 1");
        assert_eq!(toc_items[0].anchor, "title-1");
    }

    #[test]
    fn test_stats_conversion_and_serde() {
        let md = "Hello world from markdown.";
        let stats = extract_document_stats(md);
        let json = serde_json::to_string(&stats).unwrap();
        let doc_stats: DocStats = serde_json::from_str(&json).unwrap();
        assert_eq!(doc_stats.words, stats.words);
        assert_eq!(doc_stats.chars, stats.characters);
    }
}
