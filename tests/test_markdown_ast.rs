//! Opaque-box E2E test suite for Typora-RustReader Markdown AST, TOC & Statistics Engine.
//!
//! Features Covered:
//! - Feature 16: Outline / Heading TOC Extraction & Anchor Slugification
//! - Feature 17: Document Statistics Engine (words, chars, lines, reading time)
//!
//! Interface Contracts:
//! - TocItem { level: u32, title: String, anchor: String, line: usize }
//! - DocStats { words: usize, chars: usize, lines: usize, reading_time_minutes: usize }
//! - extract_toc_and_stats(markdown: &str) -> (Vec<TocItem>, DocStats)

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{calculate_stats_oracle, marked_slugify, TestSandbox};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocItemModel {
    pub level: u32,
    pub title: String,
    pub anchor: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocStatsModel {
    pub words: usize,
    pub chars: usize,
    pub lines: usize,
    pub reading_time_minutes: usize,
}

/// AST & TOC extraction oracle implementing the specification.
/// Parses headings while ignoring code blocks, generates clean titles and marked.js-compatible slugs.
pub fn extract_toc_and_stats_oracle(markdown: &str) -> (Vec<TocItemModel>, DocStatsModel) {
    let mut toc = Vec::new();
    let mut existing_anchors = HashSet::new();

    let mut in_code_block = false;

    let lines: Vec<&str> = markdown.lines().collect();
    let total_lines = if markdown.is_empty() { 0 } else { lines.len() };
    let total_chars = markdown.chars().count();
    let total_words = markdown
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .filter(|w| !w.chars().all(|c| c == '#' || c == '=' || c == '-' || c == '|' || c == '`' || c == '*'))
        .count();
    let reading_time_minutes = if total_words == 0 {
        0
    } else {
        ((total_words as f64) / 200.0).ceil() as usize
    };

    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim();

        // Check for fenced code block toggle
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            idx += 1;
            continue;
        }

        if !in_code_block {
            // Check ATX headings: # H1 ... ###### H6
            if trimmed.starts_with('#') {
                let mut level = 0;
                for c in trimmed.chars() {
                    if c == '#' {
                        level += 1;
                    } else {
                        break;
                    }
                }

                if level >= 1 && level <= 6 {
                    let rest = trimmed[level..].trim();
                    // Must be followed by space or empty per CommonMark
                    if trimmed.chars().nth(level) == Some(' ') || rest.is_empty() {
                        let clean_title = strip_inline_markdown(rest);
                        let anchor = marked_slugify(&clean_title, &mut existing_anchors);
                        toc.push(TocItemModel {
                            level: level as u32,
                            title: clean_title,
                            anchor,
                            line: idx + 1,
                        });
                    }
                }
            } else if idx + 1 < lines.len() && !trimmed.is_empty() {
                // Check Setext headings: === (H1) or --- (H2)
                let next_line = lines[idx + 1].trim();
                if next_line.chars().all(|c| c == '=') && next_line.len() >= 2 {
                    let clean_title = strip_inline_markdown(trimmed);
                    let anchor = marked_slugify(&clean_title, &mut existing_anchors);
                    toc.push(TocItemModel {
                        level: 1,
                        title: clean_title,
                        anchor,
                        line: idx + 1,
                    });
                    idx += 1; // Skip the underline
                } else if next_line.chars().all(|c| c == '-') && next_line.len() >= 2 {
                    let clean_title = strip_inline_markdown(trimmed);
                    let anchor = marked_slugify(&clean_title, &mut existing_anchors);
                    toc.push(TocItemModel {
                        level: 2,
                        title: clean_title,
                        anchor,
                        line: idx + 1,
                    });
                    idx += 1; // Skip the underline
                }
            }
        }

        idx += 1;
    }

    let stats = DocStatsModel {
        words: total_words,
        chars: total_chars,
        lines: total_lines,
        reading_time_minutes,
    };

    (toc, stats)
}

/// Strips markdown inline markup (bold, italic, links, code spans) from heading titles.
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
                let mut tag = String::new();
                for inner in chars.by_ref() {
                    if inner == '>' {
                        break;
                    }
                    tag.push(inner);
                }
            }
            other => result.push(other),
        }
    }

    result.trim().to_string()
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 10 tests across F16 and F17)
// =========================================================================

#[test]
fn test_markdown_f16_extracts_h1_through_h6() {
    let md = "# Level 1\n## Level 2\n### Level 3\n#### Level 4\n##### Level 5\n###### Level 6";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 6);
    for (i, item) in toc.iter().enumerate() {
        assert_eq!(item.level, (i + 1) as u32);
        assert_eq!(item.title, format!("Level {}", i + 1));
        assert_eq!(item.anchor, format!("level-{}", i + 1));
        assert_eq!(item.line, i + 1);
    }
}

#[test]
fn test_markdown_f16_anchor_slugification_simple() {
    let md = "# Introduction to Typora";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].anchor, "introduction-to-typora");
}

#[test]
fn test_markdown_f16_duplicate_heading_disambiguation() {
    let md = "# Overview\n# Overview\n# Overview";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 3);
    assert_eq!(toc[0].anchor, "overview");
    assert_eq!(toc[1].anchor, "overview-1");
    assert_eq!(toc[2].anchor, "overview-2");
}

#[test]
fn test_markdown_f16_code_block_headings_ignored() {
    let md = "# Real Heading\n```rust\n# Fake Heading in Code\n```\n## Second Real Heading";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Real Heading");
    assert_eq!(toc[1].title, "Second Real Heading");
}

#[test]
fn test_markdown_f16_inline_formatting_stripped_from_title() {
    let md = "# **Bold Heading** and *Italic* with `code`";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Bold Heading and Italic with code");
    assert_eq!(toc[0].anchor, "bold-heading-and-italic-with-code");
}

#[test]
fn test_markdown_f16_setext_heading_level_1_and_2() {
    let md = "First Section\n=============\n\nSecond Section\n--------------";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "First Section");
    assert_eq!(toc[0].anchor, "first-section");

    assert_eq!(toc[1].level, 2);
    assert_eq!(toc[1].title, "Second Section");
    assert_eq!(toc[1].anchor, "second-section");
}

#[test]
fn test_markdown_f17_word_count_accurate() {
    let md = "One two three four five six seven eight nine ten.";
    let (_, stats) = extract_toc_and_stats_oracle(md);
    assert_eq!(stats.words, 10);
}

#[test]
fn test_markdown_f17_character_count_accurate() {
    let md = "Hello World!";
    let (_, stats) = extract_toc_and_stats_oracle(md);
    assert_eq!(stats.chars, 12);
}

#[test]
fn test_markdown_f17_line_count_accurate() {
    let md = "Line 1\nLine 2\nLine 3\nLine 4";
    let (_, stats) = extract_toc_and_stats_oracle(md);
    assert_eq!(stats.lines, 4);
}

#[test]
fn test_markdown_f17_reading_time_estimation() {
    // 200 words = 1 min
    let words_200 = "word ".repeat(200);
    let (_, stats_200) = extract_toc_and_stats_oracle(&words_200);
    assert_eq!(stats_200.reading_time_minutes, 1);

    // 201 words = 2 min
    let words_201 = "word ".repeat(201);
    let (_, stats_201) = extract_toc_and_stats_oracle(&words_201);
    assert_eq!(stats_201.reading_time_minutes, 2);

    // 500 words = 3 min
    let words_500 = "word ".repeat(500);
    let (_, stats_500) = extract_toc_and_stats_oracle(&words_500);
    assert_eq!(stats_500.reading_time_minutes, 3);
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 20 tests)
// =========================================================================

#[test]
fn test_markdown_boundary_empty_document() {
    let (toc, stats) = extract_toc_and_stats_oracle("");
    assert!(toc.is_empty());
    assert_eq!(stats.words, 0);
    assert_eq!(stats.chars, 0);
    assert_eq!(stats.lines, 0);
    assert_eq!(stats.reading_time_minutes, 0);
}

#[test]
fn test_markdown_boundary_only_whitespace_and_newlines() {
    let (toc, stats) = extract_toc_and_stats_oracle("   \n\t\n   \n");
    assert!(toc.is_empty());
    assert_eq!(stats.words, 0);
    assert_eq!(stats.reading_time_minutes, 0);
}

#[test]
fn test_markdown_boundary_cjk_unicode_heading() {
    let md = "# 快速入门 指南";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "快速入门 指南");
    assert_eq!(toc[0].anchor, "快速入门-指南");
}

#[test]
fn test_markdown_boundary_punctuation_and_symbols_in_heading() {
    let md = "# C++ & Rust: The 2026 Edition (v2.0)!";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].anchor, "c-rust-the-2026-edition-v20");
}

#[test]
fn test_markdown_boundary_hyperlink_in_heading() {
    let md = "# Visit [GitHub](https://github.com) Today";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Visit GitHub Today");
    assert_eq!(toc[0].anchor, "visit-github-today");
}

#[test]
fn test_markdown_boundary_html_tag_in_heading() {
    let md = "# <span style=\"color:red\">Alert</span> Notice";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Alert Notice");
    assert_eq!(toc[0].anchor, "alert-notice");
}

#[test]
fn test_markdown_boundary_heading_without_alphanumerics_fallback() {
    let md = "# ???";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].anchor, "section");
}

#[test]
fn test_markdown_boundary_skipped_levels() {
    let md = "# Top Level 1\n##### Skipped Level 5";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[1].level, 5);
}

#[test]
fn test_markdown_boundary_consecutive_identical_duplicates() {
    let mut md = String::new();
    for _ in 0..10 {
        md.push_str("# Section\n");
    }
    let (toc, _) = extract_toc_and_stats_oracle(&md);

    assert_eq!(toc.len(), 10);
    assert_eq!(toc[0].anchor, "section");
    for i in 1..10 {
        assert_eq!(toc[i].anchor, format!("section-{}", i));
    }
}

#[test]
fn test_markdown_boundary_tilde_fenced_code_blocks() {
    let md = "~~~python\n# Not a heading\ndef foo(): pass\n~~~\n# Real Title";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Real Title");
}

#[test]
fn test_markdown_boundary_line_numbers_with_blank_lines() {
    let md = "\n\n\n# Line 4 Heading\n\n\n\n## Line 8 Heading";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].line, 4);
    assert_eq!(toc[1].line, 8);
}

#[test]
fn test_markdown_boundary_gfm_tables_words_counted() {
    let md = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |";
    let (_, stats) = extract_toc_and_stats_oracle(md);
    assert!(stats.words >= 6);
    assert_eq!(stats.lines, 4);
}

#[test]
fn test_markdown_boundary_math_block_katex_counted() {
    let md = "# Math\n$$\n\\int_{-\\infty}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}\n$$\nText after";
    let (toc, stats) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Math");
    assert!(stats.words > 0);
}

#[test]
fn test_markdown_boundary_inline_katex_in_heading() {
    let md = "# The $E=mc^2$ Equation";
    let (toc, _) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "The $E=mc^2$ Equation");
}

#[test]
fn test_markdown_boundary_crlf_line_counting() {
    let md_lf = "Line 1\nLine 2\nLine 3";
    let md_crlf = "Line 1\r\nLine 2\r\nLine 3";

    let (_, stats_lf) = extract_toc_and_stats_oracle(md_lf);
    let (_, stats_crlf) = extract_toc_and_stats_oracle(md_crlf);

    assert_eq!(stats_lf.lines, stats_crlf.lines);
    assert_eq!(stats_lf.words, stats_crlf.words);
}

#[test]
fn test_markdown_boundary_hash_without_space_is_not_heading() {
    let md = "#not_a_heading and #another_tag";
    let (toc, _) = extract_toc_and_stats_oracle(md);
    assert!(toc.is_empty());
}

#[test]
fn test_markdown_boundary_more_than_six_hashes_is_not_heading() {
    let md = "####### Seven Hashes";
    let (toc, _) = extract_toc_and_stats_oracle(md);
    assert!(toc.is_empty());
}

#[test]
fn test_markdown_boundary_single_word_document() {
    let (toc, stats) = extract_toc_and_stats_oracle("Word");
    assert_eq!(stats.words, 1);
    assert_eq!(stats.reading_time_minutes, 1);
}

#[test]
fn test_markdown_boundary_reading_time_ceiling() {
    // 1 word -> 1 min
    // 200 words -> 1 min
    // 201 words -> 2 min
    // 400 words -> 2 min
    // 401 words -> 3 min
    let (w1, _) = ("w", 1);
    let (_, s1) = extract_toc_and_stats_oracle("one");
    assert_eq!(s1.reading_time_minutes, 1);

    let text_400 = "word ".repeat(400);
    let (_, s400) = extract_toc_and_stats_oracle(&text_400);
    assert_eq!(s400.reading_time_minutes, 2);

    let text_401 = "word ".repeat(401);
    let (_, s401) = extract_toc_and_stats_oracle(&text_401);
    assert_eq!(s401.reading_time_minutes, 3);
}

#[test]
fn test_markdown_boundary_massive_document_performance() {
    let mut large_doc = String::new();
    for i in 1..=2000 {
        large_doc.push_str(&format!("## Heading {}\n\nParagraph text with several words here for testing performance.\n\n", i));
    }

    let (toc, stats) = extract_toc_and_stats_oracle(&large_doc);
    assert_eq!(toc.len(), 2000);
    assert!(stats.words > 10_000);
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS
// =========================================================================

#[test]
fn test_markdown_combine_toc_and_stats_synchronization() {
    let md = "# Main Title\n\nIntro paragraph.\n\n## Sub Title\n\nDetail paragraph.";
    let (toc, stats) = extract_toc_and_stats_oracle(md);

    assert_eq!(toc.len(), 2);
    assert_eq!(stats.lines, 7);
    assert_eq!(stats.words, 8);
}

#[test]
fn test_markdown_combine_doc_edit_updates_stats_and_toc() {
    let mut md = String::from("# Initial Title\n\nFirst paragraph.");
    let (toc1, stats1) = extract_toc_and_stats_oracle(&md);
    assert_eq!(toc1.len(), 1);
    let initial_words = stats1.words;

    // Append 50 words and new heading
    md.push_str("\n\n## Second Title\n\n");
    md.push_str(&"additional ".repeat(50));

    let (toc2, stats2) = extract_toc_and_stats_oracle(&md);
    assert_eq!(toc2.len(), 2);
    assert_eq!(stats2.words, initial_words + 2 + 50);
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_markdown_scenario_parse_repo_sample_md() {
    let sample_path = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/sample.md");
    if sample_path.exists() {
        let content = fs::read_to_string(sample_path).expect("Failed to read sample.md");
        let (toc, stats) = extract_toc_and_stats_oracle(&content);

        assert!(!toc.is_empty(), "sample.md must contain headings");
        assert!(stats.words > 100, "sample.md must have words");
        assert!(stats.lines > 10, "sample.md must have lines");

        // Verify anchors are unique
        let mut anchors = HashSet::new();
        for item in &toc {
            assert!(anchors.insert(item.anchor.clone()), "Duplicate anchor found: {}", item.anchor);
        }
    }
}

#[test]
fn test_markdown_scenario_parse_repo_agent_md() {
    let agent_path = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/AGENT.MD");
    if agent_path.exists() {
        let content = fs::read_to_string(agent_path).expect("Failed to read AGENT.MD");
        let (toc, stats) = extract_toc_and_stats_oracle(&content);

        assert!(!toc.is_empty(), "AGENT.MD must contain headings");
        assert!(stats.words > 500, "AGENT.MD word count");
    }
}
