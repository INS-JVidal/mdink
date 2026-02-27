//! Layout engine: flattens RenderedBlock IR into DocumentLine sequences for rendering.
//!
//! This module is the second stage of the rendering pipeline. It takes
//! the block-level IR from the parser and produces a flat sequence of
//! `DocumentLine`s sized to fit a given terminal width.

use ratatui::style::{Color, Modifier, Style};
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
    /// A line of syntax-highlighted code (no wrapping).
    Code(Line<'static>),
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
            RenderedBlock::CodeBlock {
                language,
                highlighted_lines,
            } => {
                // Emit language label header if language is specified.
                if !language.is_empty() {
                    let label = Span::styled(
                        format!(" {language} "),
                        Style::default()
                            .fg(Color::Indexed(245))
                            .bg(Color::Indexed(235))
                            .add_modifier(Modifier::ITALIC),
                    );
                    lines.push(DocumentLine::Code(Line::from(label)));
                }
                // Emit each highlighted line (no wrapping — code is literal).
                for line in highlighted_lines {
                    lines.push(DocumentLine::Code(line.clone()));
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
        let wrapped_str: &str = wrapped_text.as_ref();

        // Skip whitespace between wrapped lines (break points consumed by textwrap).
        // Only advance forward — the cursor never goes backward.
        while cursor < plain.len() {
            if plain[cursor..].starts_with(wrapped_str) {
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

        // Guard: if cursor exhausted `plain` without finding this line, textwrap
        // returned a `Cow::Owned` string with modified content (e.g. soft hyphen
        // stripped by UnicodeBreakProperties). Emitting built_spans_for_range would
        // either produce empty spans (silent data loss) or slice on a non-char
        // boundary (panic). Fall back to emitting the wrapped text directly instead.
        if cursor >= plain.len() && !plain.ends_with(wrapped_str) {
            result.push(Line::from(Span::raw(wrapped_str.to_string())));
            continue;
        }

        let line_start = cursor;
        let line_end = cursor + wrapped_str.len();
        // Clamp to plain text length for safety.
        let line_end = line_end.min(plain.len());

        // Verify the end is on a char boundary before slicing. If not (can only
        // happen with Cow::Owned from textwrap), emit the text directly.
        if !plain.is_char_boundary(line_end) {
            result.push(Line::from(Span::raw(wrapped_str.to_string())));
            cursor = line_end.min(plain.len());
            continue;
        }

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
#[path = "layout_tests.rs"]
mod tests;
