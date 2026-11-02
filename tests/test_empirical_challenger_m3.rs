//! Empirical Challenger Stress Tests for Milestone 3 Gate Verification.
//!
//! Conducts adversarial stress testing on Markdown AST & Document Statistics (`src/markdown.rs`):
//! 1. Fenced code blocks with `# Heading` (verify code block lines are never parsed as headings).
//! 2. HTML tags inside headings (`<h1>Tag</h1>`, `<span class="badge">Title</span>`) — verify tags stripped to text.
//! 3. Anchor slugification edge cases: duplicate headings (disambiguation `title`, `title-1`, `title-2`),
//!    headings without alphanumerics (fallback `section`), special punctuation, CJK and Unicode characters.
//! 4. Setext headings (level 1 `===`, level 2 `---`).
//! 5. Document statistics under extreme edge cases: empty string, 100,000 words, mixed unicode,
//!    code blocks, zero-space CJK text, line count accuracy.

use std::collections::HashSet;
use std::time::Instant;
use typora_rust_reader::markdown::{
    calculate_doc_stats, extract_document_stats, extract_toc_and_stats, marked_slugify,
    DocStats, TocItem,
};

// =========================================================================
// 1. FENCED CODE BLOCKS WITH HEADINGS
// =========================================================================

#[test]
fn test_adversarial_backtick_code_blocks_headings_ignored() {
    let md = r#"# Real Header 1

```rust
# Fake Heading in Code 1
## Fake Heading in Code 2
### Fake Heading in Code 3
```

## Real Header 2
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2, "Only real headings outside code fences should be extracted");
    assert_eq!(toc[0].title, "Real Header 1");
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].line, 1);
    assert_eq!(toc[1].title, "Real Header 2");
    assert_eq!(toc[1].level, 2);
    assert_eq!(toc[1].line, 9);
}

#[test]
fn test_adversarial_tilde_code_blocks_headings_ignored() {
    let md = r#"# Outer Title

~~~python
# Comment that looks like heading
def hello():
    # Another heading comment
    pass
~~~

### Subsequent Title
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Outer Title");
    assert_eq!(toc[1].title, "Subsequent Title");
    assert_eq!(toc[1].level, 3);
}

#[test]
fn test_adversarial_nested_code_fences_headings_ignored() {
    // 4-backtick code block enclosing a 3-backtick block with headings
    let md = r#"# Outer Doc

````markdown
```rust
# Deeply nested fake heading
## Another fake heading
```
````

# Post Doc
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Outer Doc");
    assert_eq!(toc[1].title, "Post Doc");
}

#[test]
fn test_adversarial_unclosed_code_fence_at_eof() {
    // If a code block is opened but not closed at EOF, CommonMark treats the remainder as code block.
    let md = r#"# Valid Header

```bash
# Inside unclosed code block
echo "never parsed as heading"
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1, "Unclosed code fence content should not produce headings");
    assert_eq!(toc[0].title, "Valid Header");
}

#[test]
fn test_adversarial_indented_code_blocks_headings_ignored() {
    // 4 spaces indentation creates an indented code block in CommonMark
    let md = r#"# Top Level

    # 4-space Indented Fake Heading
    def foo(): pass

## Second Level
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Top Level");
    assert_eq!(toc[1].title, "Second Level");
}

#[test]
fn test_adversarial_heading_immediately_surrounding_code_blocks() {
    let md = "# Start\n```\n# Hidden\n```\n# End";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Start");
    assert_eq!(toc[0].line, 1);
    assert_eq!(toc[1].title, "End");
    assert_eq!(toc[1].line, 5);
}

// =========================================================================
// 2. HTML TAGS INSIDE HEADINGS
// =========================================================================

#[test]
fn test_adversarial_html_span_and_badge_in_heading() {
    let md = r#"# <span class="badge badge-primary">Version 2.0</span> System Architecture"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Version 2.0 System Architecture");
    assert_eq!(toc[0].anchor, "version-20-system-architecture");
}

#[test]
fn test_adversarial_html_inline_tags_stripped_to_text() {
    let md = r#"# <b>Bold</b> and <i>Italic</i> with <code>Inline Code</code> and <a href="http://example.com">Link</a>"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Bold and Italic with Inline Code and Link");
    assert_eq!(toc[0].anchor, "bold-and-italic-with-inline-code-and-link");
}

#[test]
fn test_adversarial_html_nested_tags_inside_heading() {
    let md = r#"# <div><span style="color:red"><b>Deeply</b> Nested</span> Text</div>"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Deeply Nested Text");
    assert_eq!(toc[0].anchor, "deeply-nested-text");
}

#[test]
fn test_adversarial_heading_with_void_tags_and_breaks() {
    let md = "# First Part <br/> Second Part <img src=\"icon.png\" alt=\"icon\" /> End";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "First Part  Second Part  End");
    assert_eq!(toc[0].anchor, "first-part-second-part-end");
}

#[test]
fn test_adversarial_html_h1_inside_heading_stripped_to_text() {
    // When <h1> is used inside a Markdown heading: '# <h1>Tag</h1>'
    let md = "# <h1>Tag</h1>";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Tag", "HTML tags inside heading must be stripped to clean text");
    assert_eq!(toc[0].anchor, "tag");
}

#[test]
fn test_adversarial_raw_html_block_without_markdown_not_in_toc() {
    // In CommonMark, standalone raw HTML block <h1>Raw</h1> is not an ATX/Setext Markdown heading.
    // CommonMark requires a blank line to terminate HTML block type 6.
    let md = "<h1>Raw HTML Heading 1</h1>\n<p>Paragraph</p>\n\n# Real Markdown Heading\n";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Real Markdown Heading");
}

#[test]
fn test_adversarial_html_tags_inside_setext_heading() {
    let md = "<strong>Important</strong> Section\n===============================";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "Important Section");
    assert_eq!(toc[0].anchor, "important-section");
}

// =========================================================================
// 3. ANCHOR SLUGIFICATION EDGE CASES
// =========================================================================

#[test]
fn test_adversarial_duplicate_headings_chain_disambiguation() {
    let mut md = String::new();
    for _ in 0..15 {
        md.push_str("# Section\n\nContent paragraph.\n\n");
    }
    let (toc, _) = extract_toc_and_stats(&md);
    assert_eq!(toc.len(), 15);
    assert_eq!(toc[0].anchor, "section");
    for i in 1..15 {
        assert_eq!(toc[i].anchor, format!("section-{}", i));
    }
}

#[test]
fn test_adversarial_preempted_slug_disambiguation_collision() {
    // If a document has '# Title', '# Title-1', then another '# Title':
    // The second '# Title' must increment to 'title-2' avoiding 'title-1' collision.
    let md = r#"# Title
# Title 1
# Title
# Title
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 4);
    assert_eq!(toc[0].anchor, "title");
    assert_eq!(toc[1].anchor, "title-1");
    assert_eq!(toc[2].anchor, "title-2");
    assert_eq!(toc[3].anchor, "title-3");
}

#[test]
fn test_adversarial_non_alphanumeric_fallback_to_section() {
    let md = r#"# ???
# !!!
# @#$%^&*()
# ---
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 4);
    assert_eq!(toc[0].anchor, "section");
    assert_eq!(toc[1].anchor, "section-1");
    assert_eq!(toc[2].anchor, "section-2");
    assert_eq!(toc[3].anchor, "section-3");
}

#[test]
fn test_adversarial_existing_section_and_no_alphanumeric_collision() {
    let md = r#"# Section
# ???
# Section
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 3);
    assert_eq!(toc[0].anchor, "section");
    assert_eq!(toc[1].anchor, "section-1"); // fallback "section" -> disambiguated to section-1
    assert_eq!(toc[2].anchor, "section-2"); // third "section" -> disambiguated to section-2
}

#[test]
fn test_adversarial_special_punctuation_slugification() {
    let cases = vec![
        ("C++ & Rust: The 2026 Edition (v2.0)!", "c-rust-the-2026-edition-v20"),
        ("foo---bar___baz", "foo-bar-baz"),
        ("...Leading and Trailing Punctuation---", "leading-and-trailing-punctuation"),
        ("Slash / Backslash \\ Pipe | At @ Hash #", "slash-backslash-pipe-at-hash"),
        ("Quotes 'single' and \"double\"", "quotes-single-and-double"),
        ("Math a + b = c * d", "math-a-b-c-d"),
    ];

    for (title, expected_slug) in cases {
        let mut existing = HashSet::new();
        let slug = marked_slugify(title, &mut existing);
        assert_eq!(slug, expected_slug, "Failed for title: {}", title);
    }
}

#[test]
fn test_adversarial_cjk_unicode_anchors() {
    let cases = vec![
        ("快速入门 指南", "快速入门-指南"),
        ("日本語のタイトル (Overview)", "日本語のタイトル-overview"),
        ("한국어 제목 123", "한국어-제목-123"),
        ("1.2 架构设计与实现", "12-架构设计与实现"),
    ];

    for (title, expected_slug) in cases {
        let mut existing = HashSet::new();
        let slug = marked_slugify(title, &mut existing);
        assert_eq!(slug, expected_slug, "Failed CJK title: {}", title);
    }
}

#[test]
fn test_adversarial_cyrillic_greek_unicode_anchors() {
    let mut existing = HashSet::new();
    assert_eq!(
        marked_slugify("Привет мир", &mut existing),
        "привет-мир"
    );
    assert_eq!(
        marked_slugify("Ελληνικός Τίτλος", &mut existing),
        "ελληνικός-τίτλος"
    );
}

#[test]
fn test_adversarial_emoji_slugification() {
    let mut existing = HashSet::new();
    // Only emoji -> falls back to section
    assert_eq!(marked_slugify("🚀🔥🎉", &mut existing), "section");
    // Emoji + text -> emojis discarded, alphanumeric kept
    assert_eq!(marked_slugify("🚀 Launch v1.0", &mut existing), "launch-v10");
}

// =========================================================================
// 4. SETEXT HEADINGS
// =========================================================================

#[test]
fn test_adversarial_setext_heading_level_1_variations() {
    let md = r#"Heading One
===

Heading One Long Underline
==================================================
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "Heading One");
    assert_eq!(toc[0].anchor, "heading-one");
    assert_eq!(toc[0].line, 1);

    assert_eq!(toc[1].level, 1);
    assert_eq!(toc[1].title, "Heading One Long Underline");
    assert_eq!(toc[1].anchor, "heading-one-long-underline");
    assert_eq!(toc[1].line, 4);
}

#[test]
fn test_adversarial_setext_heading_level_2_variations() {
    let md = r#"Heading Two
---

Heading Two Long Underline
--------------------------------------------------
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].level, 2);
    assert_eq!(toc[0].title, "Heading Two");
    assert_eq!(toc[0].anchor, "heading-two");
    assert_eq!(toc[0].line, 1);

    assert_eq!(toc[1].level, 2);
    assert_eq!(toc[1].title, "Heading Two Long Underline");
    assert_eq!(toc[1].anchor, "heading-two-long-underline");
    assert_eq!(toc[1].line, 4);
}

#[test]
fn test_adversarial_setext_with_inline_formatting() {
    let md = "**Bold Title** with `code`\n============================";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "Bold Title with code");
    assert_eq!(toc[0].anchor, "bold-title-with-code");
    assert_eq!(toc[0].line, 1);
}

#[test]
fn test_adversarial_setext_multiline_heading() {
    let md = "First line of heading\nSecond line of heading\n======================";
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "First line of heading Second line of heading");
    assert_eq!(toc[0].anchor, "first-line-of-heading-second-line-of-heading");
    assert_eq!(toc[0].line, 1);
}

#[test]
fn test_adversarial_setext_inside_code_fence_ignored() {
    let md = r#"```markdown
Not a Setext Heading
====================
```
Real Setext
===========
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Real Setext");
    assert_eq!(toc[0].line, 5);
}

// =========================================================================
// 5. DOCUMENT STATISTICS UNDER EXTREME EDGE CASES
// =========================================================================

#[test]
fn test_adversarial_stats_empty_string() {
    let stats = extract_document_stats("");
    assert_eq!(stats.words, 0);
    assert_eq!(stats.characters, 0);
    assert_eq!(stats.characters_no_spaces, 0);
    assert_eq!(stats.lines, 0);
    assert_eq!(stats.reading_time_minutes, 0);

    let doc_stats = calculate_doc_stats("");
    assert_eq!(doc_stats.words, 0);
    assert_eq!(doc_stats.chars, 0);
    assert_eq!(doc_stats.lines, 0);
    assert_eq!(doc_stats.reading_time_minutes, 0);
}

#[test]
fn test_adversarial_stats_whitespace_only() {
    let input = "   \t\t\n\r\n   \t  \n";
    let stats = extract_document_stats(input);
    assert_eq!(stats.words, 0);
    assert_eq!(stats.reading_time_minutes, 0);
    assert_eq!(stats.characters_no_spaces, 0);
    assert!(stats.characters > 0);
    assert!(stats.lines > 0);
}

#[test]
fn test_adversarial_stats_100_000_words_stress_and_performance() {
    // Generate 100,000 words
    let mut large_doc = String::with_capacity(100_000 * 7);
    for i in 0..100_000 {
        large_doc.push_str("word");
        if i % 10 == 9 {
            large_doc.push('\n');
        } else {
            large_doc.push(' ');
        }
    }

    let start = Instant::now();
    let stats = extract_document_stats(&large_doc);
    let duration = start.elapsed();

    assert_eq!(stats.words, 100_000);
    assert_eq!(stats.lines, 10_000);
    // 100,000 / 200 = 500 minutes
    assert_eq!(stats.reading_time_minutes, 500);
    // Processing 100k words must complete in under 500ms
    assert!(
        duration.as_millis() < 500,
        "Stress test took too long: {:?}",
        duration
    );
}

#[test]
fn test_adversarial_stats_mixed_unicode_character_count() {
    // Emojis (4 bytes each) and CJK (3 bytes each) and ASCII (1 byte each)
    let text = "Hello 世界 🚀🎉";
    let stats = extract_document_stats(text);
    // Characters: 'H','e','l','l','o',' ','世','界',' ','🚀','🎉' = 11 characters
    assert_eq!(stats.characters, 11);
    // Non-whitespace: 11 - 2 spaces = 9
    assert_eq!(stats.characters_no_spaces, 9);
    // Words: "Hello", "世界", "🚀🎉" = 3 tokens
    assert_eq!(stats.words, 3);
}

#[test]
fn test_adversarial_stats_zero_space_cjk_character_accuracy() {
    let cjk_text = "这是一段完全没有空格的简体中文测试文本。";
    let stats = extract_document_stats(cjk_text);
    let expected_chars = cjk_text.chars().count();
    assert_eq!(stats.characters, expected_chars);
    assert_eq!(stats.characters_no_spaces, expected_chars);
    assert_eq!(stats.lines, 1);
}

#[test]
fn test_adversarial_stats_code_blocks_and_delimiters_filtering() {
    let md = r#"# Heading Delimiter

```rust
fn main() {
    println!("Hello, world!");
}
```

- list item 1
* list item 2
===
---
"#;
    let (_, stats) = extract_toc_and_stats(md);
    // Delimiters like '#', '===', '---', '*', '-' are excluded from word count
    assert!(stats.words > 0);
    assert_eq!(stats.lines, 12);
}

#[test]
fn test_adversarial_stats_line_count_accuracy() {
    assert_eq!(extract_document_stats("Single line").lines, 1);
    assert_eq!(extract_document_stats("Single line with newline\n").lines, 1);
    assert_eq!(extract_document_stats("Line 1\nLine 2\nLine 3").lines, 3);
    assert_eq!(extract_document_stats("Line 1\r\nLine 2\r\nLine 3").lines, 3);
    assert_eq!(extract_document_stats("\n\n\n").lines, 3);
}

#[test]
fn test_adversarial_reading_time_bounds() {
    // 0 words -> 0 min
    assert_eq!(extract_document_stats("").reading_time_minutes, 0);

    // 1 word -> 1 min (ceil)
    assert_eq!(extract_document_stats("word").reading_time_minutes, 1);

    // 200 words -> 1 min
    let w200 = "word ".repeat(200);
    assert_eq!(extract_document_stats(&w200).reading_time_minutes, 1);

    // 201 words -> 2 min
    let w201 = "word ".repeat(201);
    assert_eq!(extract_document_stats(&w201).reading_time_minutes, 2);

    // 400 words -> 2 min
    let w400 = "word ".repeat(400);
    assert_eq!(extract_document_stats(&w400).reading_time_minutes, 2);

    // 401 words -> 3 min
    let w401 = "word ".repeat(401);
    assert_eq!(extract_document_stats(&w401).reading_time_minutes, 3);
}

// =========================================================================
// 6. SERDE & INTEROP ROUNDTRIP TESTS
// =========================================================================

#[test]
fn test_adversarial_toc_and_stats_serde_json_compatibility() {
    let md = "# Main Title\n\nSome introductory text.";
    let (toc, stats) = extract_toc_and_stats(md);

    let toc_json = serde_json::to_string(&toc).expect("Serialize TOC");
    let stats_json = serde_json::to_string(&stats).expect("Serialize stats");

    let toc_back: Vec<TocItem> = serde_json::from_str(&toc_json).expect("Deserialize TOC");
    let stats_back: DocStats = serde_json::from_str(&stats_json).expect("Deserialize stats");

    assert_eq!(toc, toc_back);
    assert_eq!(stats, stats_back);
}
