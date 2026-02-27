//! Layout engine: flattens RenderedBlock IR into DocumentLine sequences for rendering.
//!
//! This module is the second stage of the rendering pipeline. It takes
//! the block-level IR from the parser and produces a flat sequence of
//! `DocumentLine`s sized to fit a given terminal width.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::parser::{RenderedBlock, StyledSpan};

/// A pre-rendered document ready for viewport slicing and rendering.
///
/// Contains all lines laid out for a specific terminal width. Created
/// once on load and again on terminal resize.
pub struct PreRenderedDocument {
    /// All document lines in display order.
    pub lines: Vec<DocumentLine>,
    /// Total number of lines (== `lines.len()`).
    pub total_height: usize,
}

/// A single line of the pre-rendered document.
///
/// The renderer matches on this enum exhaustively to produce frame output.
pub enum DocumentLine {
    /// A line of styled text (paragraph, heading, etc.).
    Text(Line<'static>),
    /// An empty line used for inter-block spacing.
    Empty,
    /// A horizontal rule spanning the terminal width.
    Rule,
}

/// Flattens a sequence of `RenderedBlock`s into a `PreRenderedDocument`.
///
/// Each block is converted to one or more `DocumentLine`s. Text blocks
/// are word-wrapped to fit within `width` columns. An `Empty` line is
/// inserted between adjacent blocks for visual spacing.
pub fn flatten(blocks: &[RenderedBlock], width: u16) -> PreRenderedDocument {
    let mut lines: Vec<DocumentLine> = Vec::new();
    // Clamp to minimum width of 1 to avoid undefined textwrap behavior.
    let width = (width as usize).max(1);

    for (i, block) in blocks.iter().enumerate() {
        // Inter-block spacing (not before the first block).
        if i > 0 {
            lines.push(DocumentLine::Empty);
        }

        match block {
            RenderedBlock::Heading { content, .. } => {
                let wrapped = wrap_styled_spans(content, width);
                if wrapped.is_empty() {
                    lines.push(DocumentLine::Empty);
                } else {
                    for line in wrapped {
                        lines.push(DocumentLine::Text(line));
                    }
                }
            }
            RenderedBlock::Paragraph { content } => {
                let wrapped = wrap_styled_spans(content, width);
                if wrapped.is_empty() {
                    lines.push(DocumentLine::Empty);
                } else {
                    for line in wrapped {
                        lines.push(DocumentLine::Text(line));
                    }
                }
            }
            RenderedBlock::ThematicBreak => {
                lines.push(DocumentLine::Rule);
            }
            RenderedBlock::Spacer { lines: count } => {
                for _ in 0..*count {
                    lines.push(DocumentLine::Empty);
                }
            }
        }
    }

    let total_height = lines.len();
    PreRenderedDocument {
        lines,
        total_height,
    }
}

/// Wraps styled spans to fit within a given width, preserving styles.
///
/// Algorithm:
/// 1. Concatenate all span text into a single plain-text string, building
///    a parallel byte-to-style map.
/// 2. Use `textwrap::wrap()` to determine line break positions.
/// 3. Walk a cursor through the plain text for each wrapped line, skipping
///    whitespace break points, then extract styled spans by consulting
///    the byte-to-style map.
fn wrap_styled_spans(spans: &[StyledSpan], width: usize) -> Vec<Line<'static>> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Handle hard breaks (\n) by splitting into sub-paragraphs.
    if spans.iter().any(|s| s.text.contains('\n')) {
        return wrap_with_hard_breaks(spans, width);
    }

    // 1. Build plain text and parallel byte-to-style map.
    let mut plain = String::new();
    let mut byte_styles: Vec<Style> = Vec::new();
    for span in spans {
        for _ in span.text.bytes() {
            byte_styles.push(span.style);
        }
        plain.push_str(&span.text);
    }

    if plain.is_empty() {
        return Vec::new();
    }

    // 2. Wrap the plain text.
    let wrap_options = textwrap::Options::new(width)
        .word_separator(textwrap::WordSeparator::UnicodeBreakProperties);
    let wrapped_lines = textwrap::wrap(&plain, &wrap_options);

    // 3. Map each wrapped line back to styled spans using a monotonic cursor.
    let mut result = Vec::with_capacity(wrapped_lines.len());
    let mut cursor: usize = 0;

    for wrapped_text in &wrapped_lines {
        // Skip whitespace between wrapped lines (break points consumed by textwrap).
        // Only advance forward — the cursor never goes backward.
        while cursor < plain.len() {
            if plain[cursor..].starts_with(wrapped_text.as_ref()) {
                break;
            }
            // Advance by one character (not one byte) to stay on char boundaries.
            let ch_len = plain[cursor..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
            cursor += ch_len;
        }

        let line_start = cursor;
        let line_end = cursor + wrapped_text.len();
        // Clamp to plain text length for safety.
        let line_end = line_end.min(plain.len());

        let line_spans = build_spans_for_range(&plain, &byte_styles, line_start, line_end);
        result.push(Line::from(line_spans));

        cursor = line_end;
    }

    result
}

/// Builds styled `Span`s for a byte range of the plain text.
///
/// Walks through the range by characters, grouping consecutive bytes
/// that share the same style into a single `Span`. All slicing happens
/// at character boundaries.
fn build_spans_for_range(
    plain: &str,
    byte_styles: &[Style],
    start: usize,
    end: usize,
) -> Vec<Span<'static>> {
    if start >= end || start >= plain.len() {
        return Vec::new();
    }

    let segment = &plain[start..end];
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run_start = start;
    let mut run_style = byte_styles[start];

    for (i, _ch) in segment.char_indices() {
        let abs_pos = start + i;
        if byte_styles[abs_pos] != run_style {
            let text = &plain[run_start..abs_pos];
            if !text.is_empty() {
                spans.push(Span::styled(text.to_string(), run_style));
            }
            run_start = abs_pos;
            run_style = byte_styles[abs_pos];
        }
    }

    // Emit final run.
    let text = &plain[run_start..end];
    if !text.is_empty() {
        spans.push(Span::styled(text.to_string(), run_style));
    }

    spans
}

/// Handles text containing hard breaks by splitting at `\n` boundaries
/// first, then wrapping each segment independently.
fn wrap_with_hard_breaks(spans: &[StyledSpan], width: usize) -> Vec<Line<'static>> {
    let mut groups: Vec<Vec<StyledSpan>> = Vec::new();
    let mut current_group: Vec<StyledSpan> = Vec::new();

    for span in spans {
        if span.text.contains('\n') {
            let parts: Vec<&str> = span.text.split('\n').collect();
            for (i, part) in parts.iter().enumerate() {
                if !part.is_empty() {
                    current_group.push(StyledSpan {
                        text: part.to_string(),
                        style: span.style,
                    });
                }
                if i < parts.len() - 1 {
                    groups.push(std::mem::take(&mut current_group));
                }
            }
        } else {
            current_group.push(StyledSpan {
                text: span.text.clone(),
                style: span.style,
            });
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    let mut result = Vec::new();
    for group in &groups {
        let wrapped = wrap_styled_spans(group, width);
        if wrapped.is_empty() {
            result.push(Line::from(Vec::<Span<'static>>::new()));
        } else {
            result.extend(wrapped);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::StyledSpan;
    use ratatui::style::{Color, Modifier, Style};

    fn plain_span(text: &str) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            style: Style::default(),
        }
    }

    fn styled_span(text: &str, style: Style) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            style,
        }
    }

    #[test]
    fn test_layout_empty_blocks() {
        let doc = flatten(&[], 80);
        assert_eq!(doc.total_height, 0);
        assert!(doc.lines.is_empty());
    }

    #[test]
    fn test_layout_single_paragraph_no_wrap() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("Hello world")],
        }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
        assert!(matches!(&doc.lines[0], DocumentLine::Text(_)));
    }

    #[test]
    fn test_layout_paragraph_wraps_at_width() {
        let long_text = "word ".repeat(20); // 100 chars
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span(long_text.trim())],
        }];
        let doc = flatten(&blocks, 40);
        assert!(
            doc.total_height > 1,
            "expected wrapping, got {} lines",
            doc.total_height
        );
    }

    #[test]
    fn test_layout_thematic_break() {
        let blocks = vec![RenderedBlock::ThematicBreak];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
        assert!(matches!(&doc.lines[0], DocumentLine::Rule));
    }

    #[test]
    fn test_layout_inter_block_spacing() {
        let blocks = vec![
            RenderedBlock::Paragraph {
                content: vec![plain_span("First")],
            },
            RenderedBlock::Paragraph {
                content: vec![plain_span("Second")],
            },
        ];
        let doc = flatten(&blocks, 80);
        // First paragraph (1 line) + empty (1 line) + second paragraph (1 line) = 3
        assert_eq!(doc.total_height, 3);
        assert!(matches!(&doc.lines[1], DocumentLine::Empty));
    }

    #[test]
    fn test_layout_heading_renders_as_text() {
        let blocks = vec![RenderedBlock::Heading {
            level: 1,
            content: vec![styled_span(
                "Title",
                Style::default().add_modifier(Modifier::BOLD),
            )],
        }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
        assert!(matches!(&doc.lines[0], DocumentLine::Text(_)));
    }

    #[test]
    fn test_layout_spacer() {
        let blocks = vec![RenderedBlock::Spacer { lines: 3 }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 3);
        for line in &doc.lines {
            assert!(matches!(line, DocumentLine::Empty));
        }
    }

    #[test]
    fn test_layout_single_long_word() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("abcdefghijklmnopqrstuvwxyz")],
        }];
        let doc = flatten(&blocks, 10);
        assert!(doc.total_height >= 2, "long word should wrap");
    }

    #[test]
    fn test_layout_empty_paragraph() {
        let blocks = vec![RenderedBlock::Paragraph { content: vec![] }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
    }

    #[test]
    fn test_layout_preserves_styles_across_wrap() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let text = "word ".repeat(20);
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![styled_span(text.trim(), bold)],
        }];
        let doc = flatten(&blocks, 40);
        for line in &doc.lines {
            if let DocumentLine::Text(l) = line {
                for span in &l.spans {
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "style lost after wrapping"
                    );
                }
            }
        }
    }

    #[test]
    fn test_layout_repeated_text_no_misalignment() {
        // Regression test: repeated text must not confuse the cursor.
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("aaa bbb aaa bbb aaa bbb")],
        }];
        let doc = flatten(&blocks, 8);
        // Collect all text from the wrapped lines.
        let mut all_text = String::new();
        for line in &doc.lines {
            if let DocumentLine::Text(l) = line {
                for span in &l.spans {
                    all_text.push_str(&span.content);
                }
                all_text.push(' '); // represent line break as space
            }
        }
        // All original words must appear (no duplication, no loss).
        assert_eq!(all_text.matches("aaa").count(), 3, "word 'aaa' count");
        assert_eq!(all_text.matches("bbb").count(), 3, "word 'bbb' count");
    }

    #[test]
    fn test_layout_multi_style_spans_across_wrap() {
        // Two styled spans that together exceed the width.
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let italic = Style::default().add_modifier(Modifier::ITALIC);
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![
                styled_span("hello ", bold),
                styled_span("world this is long", italic),
            ],
        }];
        let doc = flatten(&blocks, 12);
        assert!(doc.total_height >= 2, "should wrap");
        // First line should have bold "hello " and italic "world"
        if let DocumentLine::Text(first_line) = &doc.lines[0] {
            assert!(!first_line.spans.is_empty(), "first line should have spans");
        }
    }

    #[test]
    fn test_layout_unicode_emoji_no_panic() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("Hello 🌍 world 🎉 test 🚀 more text here for wrapping")],
        }];
        // Should not panic on emoji at any width.
        let doc = flatten(&blocks, 15);
        assert!(doc.total_height >= 1);
    }

    #[test]
    fn test_layout_cjk_text_no_panic() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("日本語のテキスト処理テスト")],
        }];
        let doc = flatten(&blocks, 10);
        assert!(doc.total_height >= 1);
    }

    #[test]
    fn test_layout_zero_width_no_panic() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("text")],
        }];
        // Width 0 is clamped to 1 — should not panic.
        let doc = flatten(&blocks, 0);
        assert!(doc.total_height >= 1);
    }

    #[test]
    fn test_layout_mixed_styles_content_preserved() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let code = Style::default().fg(Color::Indexed(252)).bg(Color::Indexed(236));
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![
                styled_span("Use ", Style::default()),
                styled_span("fmt", code),
                styled_span(" for formatting output in your programs", bold),
            ],
        }];
        let doc = flatten(&blocks, 20);
        // Collect all text.
        let mut all_text = String::new();
        for line in &doc.lines {
            if let DocumentLine::Text(l) = line {
                for span in &l.spans {
                    all_text.push_str(&span.content);
                }
            }
        }
        assert!(all_text.contains("Use "), "should contain 'Use '");
        assert!(all_text.contains("fmt"), "should contain 'fmt'");
        assert!(
            all_text.contains("formatting"),
            "should contain 'formatting'"
        );
    }
}
