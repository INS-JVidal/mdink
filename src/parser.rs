//! Markdown parser: converts pulldown-cmark events into the RenderedBlock IR.
//!
//! This module is the first stage of the rendering pipeline. It consumes
//! a markdown source string and produces a `Vec<RenderedBlock>` — the
//! intermediate representation consumed by the layout engine.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};

/// A rendered markdown block ready for layout.
///
/// Each variant corresponds to a markdown block-level element.
/// Inline styling is carried via `Vec<StyledSpan>` in content fields.
// IR variants and fields are forward-declared for later phases; allow unused.
#[allow(dead_code)]
pub enum RenderedBlock {
    /// Heading with level (1–6). Content carries inline styles.
    Heading { level: u8, content: Vec<StyledSpan> },
    /// A paragraph of text with inline formatting.
    Paragraph { content: Vec<StyledSpan> },
    /// A horizontal rule / thematic break.
    ThematicBreak,
    /// Vertical spacing between blocks.
    Spacer { lines: u16 },
}

/// A text span with associated style information.
///
/// Multiple `StyledSpan`s compose a line of styled text. Each span
/// carries a contiguous run of text sharing the same `ratatui::Style`.
pub struct StyledSpan {
    /// The text content of this span.
    pub text: String,
    /// The ratatui style to apply when rendering.
    pub style: Style,
}

/// Parser state machine states.
///
/// Tracks what block-level element we are currently inside. Events are
/// interpreted differently depending on the active state.
enum ParserState {
    /// Not inside any block — waiting for the next block-level start event.
    TopLevel,
    /// Inside a heading block; `level` is 1–6.
    InHeading { level: u8 },
    /// Inside a paragraph block.
    InParagraph,
    /// Inside an unrecognized block that we skip in this phase.
    /// We count nesting depth so we know when the matching End arrives.
    Skipping { depth: u32 },
}

/// Returns the default heading style for a given level (1–6).
///
/// Centralized here as the single swap point for Phase 5 theming.
fn default_heading_style(level: u8) -> Style {
    let color = match level {
        1 => Color::LightCyan,
        2 => Color::Green,
        3 => Color::Yellow,
        // h4–h6 all use white
        _ => Color::White,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// Returns the default inline code style.
///
/// Dark gray background with light gray foreground.
fn default_code_style() -> Style {
    Style::default()
        .bg(Color::Indexed(236))
        .fg(Color::Indexed(252))
}

/// Computes the effective style by merging the current base style with
/// all active inline modifiers from the style stack.
fn effective_style(style_stack: &[Style]) -> Style {
    style_stack
        .iter()
        .fold(Style::default(), |acc, s| acc.patch(*s))
}

/// Converts a pulldown-cmark `HeadingLevel` to a `u8` (1–6).
fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Parses a markdown source string into the RenderedBlock IR.
///
/// Enables GFM extensions (strikethrough, tables, tasklists) so that
/// user markdown containing these features doesn't break — even though
/// tables and lists aren't rendered until later phases.
pub fn parse(source: &str) -> Vec<RenderedBlock> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options);

    let mut blocks: Vec<RenderedBlock> = Vec::new();
    let mut state_stack: Vec<ParserState> = vec![ParserState::TopLevel];
    let mut style_stack: Vec<Style> = Vec::new();
    let mut current_spans: Vec<StyledSpan> = Vec::new();

    for event in parser {
        let current_state = state_stack.last().expect("state stack must never be empty");

        // If we're skipping an unrecognized block, handle nesting depth.
        if let ParserState::Skipping { depth } = current_state {
            match event {
                Event::Start(_) => {
                    let new_depth = depth + 1;
                    // Pop old Skipping, push new with incremented depth
                    state_stack.pop();
                    state_stack.push(ParserState::Skipping { depth: new_depth });
                }
                Event::End(_) => {
                    if *depth == 0 {
                        state_stack.pop();
                    } else {
                        let new_depth = depth - 1;
                        state_stack.pop();
                        state_stack.push(ParserState::Skipping { depth: new_depth });
                    }
                }
                // Consume all other events while skipping.
                _ => {}
            }
            continue;
        }

        match event {
            // ── Block-level start events ────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = heading_level_to_u8(level);
                style_stack.push(default_heading_style(lvl));
                current_spans.clear();
                state_stack.push(ParserState::InHeading { level: lvl });
            }
            Event::Start(Tag::Paragraph) => {
                current_spans.clear();
                state_stack.push(ParserState::InParagraph);
            }

            // ── Block-level end events ──────────────────────────────
            Event::End(TagEnd::Heading(_)) => {
                style_stack.pop();
                let level = match state_stack.pop() {
                    Some(ParserState::InHeading { level }) => level,
                    _ => 1,
                };
                let content = std::mem::take(&mut current_spans);
                blocks.push(RenderedBlock::Heading { level, content });
            }
            Event::End(TagEnd::Paragraph) => {
                state_stack.pop();
                let content = std::mem::take(&mut current_spans);
                blocks.push(RenderedBlock::Paragraph { content });
            }

            // ── Inline formatting start ─────────────────────────────
            Event::Start(Tag::Emphasis) => {
                style_stack.push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Event::Start(Tag::Strong) => {
                style_stack.push(Style::default().add_modifier(Modifier::BOLD));
            }
            Event::Start(Tag::Strikethrough) => {
                style_stack.push(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }

            // ── Inline formatting end ───────────────────────────────
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                style_stack.pop();
            }

            // ── Text content ────────────────────────────────────────
            Event::Text(text) => {
                let style = effective_style(&style_stack);
                current_spans.push(StyledSpan {
                    text: text.to_string(),
                    style,
                });
            }

            // ── Inline code ─────────────────────────────────────────
            Event::Code(text) => {
                current_spans.push(StyledSpan {
                    text: text.to_string(),
                    style: default_code_style(),
                });
            }

            // ── Breaks ──────────────────────────────────────────────
            Event::SoftBreak => {
                let style = effective_style(&style_stack);
                current_spans.push(StyledSpan {
                    text: " ".to_string(),
                    style,
                });
            }
            Event::HardBreak => {
                let style = effective_style(&style_stack);
                current_spans.push(StyledSpan {
                    text: "\n".to_string(),
                    style,
                });
            }

            // ── Thematic break (horizontal rule) ────────────────────
            Event::Rule => {
                blocks.push(RenderedBlock::ThematicBreak);
            }

            // ── Unrecognized block-level start → skip gracefully ────
            Event::Start(_) => {
                state_stack.push(ParserState::Skipping { depth: 0 });
            }

            // ── All other events (End for skipped, TaskListMarker, etc.) ──
            _ => {}
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_heading_h1_produces_heading_block() {
        let blocks = parse("# Hello");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Heading { level, content } => {
                assert_eq!(*level, 1);
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "Hello");
            }
            _ => panic!("expected Heading block"),
        }
    }

    #[test]
    fn test_parser_heading_all_levels() {
        for lvl in 1..=6 {
            let md = format!("{} Level {}", "#".repeat(lvl), lvl);
            let blocks = parse(&md);
            assert_eq!(blocks.len(), 1, "level {lvl}");
            match &blocks[0] {
                RenderedBlock::Heading { level, .. } => {
                    assert_eq!(*level, lvl as u8, "level {lvl}");
                }
                _ => panic!("expected Heading at level {lvl}"),
            }
        }
    }

    #[test]
    fn test_parser_paragraph_plain_text() {
        let blocks = parse("Hello world");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "Hello world");
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_bold_text() {
        let blocks = parse("**bold**");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "bold");
                assert!(content[0].style.add_modifier.contains(Modifier::BOLD));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_italic_text() {
        let blocks = parse("*italic*");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "italic");
                assert!(content[0].style.add_modifier.contains(Modifier::ITALIC));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_strikethrough_text() {
        let blocks = parse("~~struck~~");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "struck");
                assert!(content[0]
                    .style
                    .add_modifier
                    .contains(Modifier::CROSSED_OUT));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_nested_bold_italic() {
        let blocks = parse("***bold italic***");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "bold italic");
                let mods = content[0].style.add_modifier;
                assert!(mods.contains(Modifier::BOLD));
                assert!(mods.contains(Modifier::ITALIC));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_inline_code() {
        let blocks = parse("Use `fmt` here");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 3);
                assert_eq!(content[0].text, "Use ");
                assert_eq!(content[1].text, "fmt");
                assert_eq!(content[1].style, default_code_style());
                assert_eq!(content[2].text, " here");
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_thematic_break() {
        let blocks = parse("---");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], RenderedBlock::ThematicBreak));
    }

    #[test]
    fn test_parser_soft_break() {
        let blocks = parse("line one\nline two");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                // Text, SoftBreak(space), Text
                assert_eq!(content.len(), 3);
                assert_eq!(content[0].text, "line one");
                assert_eq!(content[1].text, " ");
                assert_eq!(content[2].text, "line two");
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_hard_break() {
        let blocks = parse("line one\\\nline two");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert!(content.iter().any(|s| s.text == "\n"));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_empty_input() {
        let blocks = parse("");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_parser_heading_styles_are_distinct() {
        let s1 = default_heading_style(1);
        let s2 = default_heading_style(2);
        let s3 = default_heading_style(3);
        assert_ne!(s1.fg, s2.fg);
        assert_ne!(s2.fg, s3.fg);
    }

    #[test]
    fn test_parser_skips_unrecognized_blocks() {
        // Code block is unrecognized in Phase 1 — should not panic.
        let md = "```rust\nfn main() {}\n```\n\nAfter code";
        let blocks = parse(md);
        // Should have at least the paragraph after the code block.
        assert!(blocks.iter().any(|b| matches!(b, RenderedBlock::Paragraph { .. })));
    }
}
