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
    let width = width as usize;

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
/// 1. Concatenate all span text into a single plain-text string.
/// 2. Use `textwrap::wrap()` to determine line break positions.
/// 3. Map each wrapped line back to styled `Span`s by walking a cursor
///    through the original `StyledSpan`s and splitting at boundaries.
fn wrap_styled_spans(spans: &[StyledSpan], width: usize) -> Vec<Line<'static>> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Handle hard breaks (\n) by splitting into sub-paragraphs.
    // Check if any span contains a newline.
    let has_hard_break = spans.iter().any(|s| s.text.contains('\n'));

    if has_hard_break {
        return wrap_with_hard_breaks(spans, width);
    }

    // 1. Build the plain-text string.
    let plain: String = spans.iter().map(|s| s.text.as_str()).collect();

    if plain.is_empty() {
        return Vec::new();
    }

    // 2. Wrap the plain text.
    let wrap_options = textwrap::Options::new(width)
        .word_separator(textwrap::WordSeparator::UnicodeBreakProperties);
    let wrapped_lines = textwrap::wrap(&plain, &wrap_options);

    // 3. Map each wrapped line back to styled spans.
    let mut result = Vec::with_capacity(wrapped_lines.len());
    let mut char_offset: usize = 0;

    for wrapped_text in &wrapped_lines {
        let line_spans = extract_styled_line(spans, &mut char_offset, wrapped_text);
        result.push(Line::from(line_spans));
    }

    result
}

/// Handles text containing hard breaks by splitting at `\n` boundaries
/// first, then wrapping each segment independently.
fn wrap_with_hard_breaks(spans: &[StyledSpan], width: usize) -> Vec<Line<'static>> {
    // Split spans at \n boundaries into groups, then wrap each group.
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

/// Extracts styled `Span`s for a single wrapped line by walking through
/// the original spans from `char_offset`.
///
/// Updates `char_offset` in place to point past the consumed characters
/// (including any whitespace eaten by the wrapping algorithm).
fn extract_styled_line(
    spans: &[StyledSpan],
    char_offset: &mut usize,
    wrapped_text: &str,
) -> Vec<Span<'static>> {
    let mut line_spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = wrapped_text.len();

    // Skip leading whitespace that textwrap trimmed from the start of continuation lines.
    skip_consumed_whitespace(spans, char_offset, wrapped_text);

    // Walk through original spans and extract the portion that belongs to this line.
    let mut span_idx = 0;
    let mut local_offset = 0;

    // Find which span and position within it corresponds to char_offset.
    let mut cumulative = 0;
    for (i, span) in spans.iter().enumerate() {
        if cumulative + span.text.len() > *char_offset {
            span_idx = i;
            local_offset = *char_offset - cumulative;
            break;
        }
        cumulative += span.text.len();
        if i == spans.len() - 1 {
            span_idx = spans.len();
        }
    }

    while remaining > 0 && span_idx < spans.len() {
        let span = &spans[span_idx];
        let available = &span.text[local_offset..];
        let take = remaining.min(available.len());
        let segment = &available[..take];

        if !segment.is_empty() {
            line_spans.push(Span::styled(segment.to_string(), ratatui_style(span.style)));
        }

        *char_offset += take;
        remaining -= take;
        local_offset = 0;
        span_idx += 1;
    }

    // Skip trailing whitespace/space that textwrap consumed as a break point.
    skip_break_whitespace(spans, char_offset);

    line_spans
}

/// Skips whitespace at the current char_offset that textwrap consumed
/// as a line break point (the space between words where wrapping occurs).
fn skip_break_whitespace(spans: &[StyledSpan], char_offset: &mut usize) {
    let mut cumulative = 0;
    for span in spans {
        let span_end = cumulative + span.text.len();
        if *char_offset >= cumulative && *char_offset < span_end {
            let local = *char_offset - cumulative;
            if local < span.text.len() {
                let ch = span.text.as_bytes()[local];
                if ch == b' ' {
                    *char_offset += 1;
                }
            }
            return;
        }
        cumulative = span_end;
    }
}

/// Adjusts char_offset to account for leading whitespace that textwrap
/// stripped from a continuation line.
fn skip_consumed_whitespace(spans: &[StyledSpan], char_offset: &mut usize, wrapped_text: &str) {
    // textwrap preserves the text content but may strip leading spaces
    // from continuation lines. We need to find where wrapped_text starts
    // in the original text.
    let plain: String = spans.iter().map(|s| s.text.as_str()).collect();
    if *char_offset < plain.len() && !wrapped_text.is_empty() {
        // Find the wrapped text starting from char_offset
        if let Some(pos) = plain[*char_offset..].find(wrapped_text) {
            *char_offset += pos;
        }
    }
}

/// Converts our `ratatui::style::Style` to itself (identity — both are
/// the same type, but this function exists as a documentation marker
/// showing that `StyledSpan.style` is already a ratatui `Style`).
fn ratatui_style(style: Style) -> Style {
    // StyledSpan already uses ratatui::style::Style. In Phase 5, this
    // may become a conversion from theme types to ratatui types.
    style
}

/// Converts a `Style` to a heading-prefixed line (adds a leading marker).
///
/// Currently unused — headings are rendered the same as paragraphs but
/// with different styles applied at parse time.
#[allow(dead_code)]
fn heading_prefix(level: u8) -> String {
    "#".repeat(level as usize) + " "
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::StyledSpan;
    use ratatui::style::{Modifier, Style};

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
        // Should produce more than 1 line when wrapping at 40 cols.
        assert!(doc.total_height > 1, "expected wrapping, got {} lines", doc.total_height);
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
        // A single word longer than the width — textwrap will break it.
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
        // Empty content should produce an empty line.
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
        // Verify all lines have styled spans.
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
}
