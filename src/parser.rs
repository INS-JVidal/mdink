//! Markdown parser: converts pulldown-cmark events into the RenderedBlock IR.
//!
//! This module is the first stage of the rendering pipeline. It consumes
//! a markdown source string and produces a `Vec<RenderedBlock>` — the
//! intermediate representation consumed by the layout engine.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

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
    /// A fenced or indented code block with syntax highlighting.
    CodeBlock {
        /// Language from the fence info string (empty for indented/unfenced).
        language: String,
        /// Pre-highlighted lines ready for layout.
        highlighted_lines: Vec<Line<'static>>,
    },
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
    /// Inside a fenced or indented code block; accumulating text.
    InCodeBlock { language: String, buffer: String },
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
    let modifier = match level {
        1..=3 => Modifier::BOLD,
        _ => Modifier::BOLD | Modifier::ITALIC,
    };
    Style::default().fg(color).add_modifier(modifier)
}

/// Returns the default inline code style.
///
/// Dark gray background with light gray foreground.
fn default_code_style() -> Style {
    Style::default()
        .bg(Color::Indexed(236))
        .fg(Color::Indexed(252))
        .add_modifier(Modifier::BOLD | Modifier::ITALIC)
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
pub fn parse(source: &str, highlighter: &crate::highlight::Highlighter) -> Vec<RenderedBlock> {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options);

    let mut blocks: Vec<RenderedBlock> = Vec::new();
    let mut state_stack: Vec<ParserState> = vec![ParserState::TopLevel];
    let mut style_stack: Vec<Style> = Vec::new();
    let mut current_spans: Vec<StyledSpan> = Vec::new();

    for event in parser {
        // If inside a code block, accumulate text into the buffer.
        // This is checked first (before other state checks) with its own scope
        // to avoid overlapping borrows on `state_stack`.
        if matches!(state_stack.last(), Some(ParserState::InCodeBlock { .. })) {
            match event {
                Event::Text(text) => {
                    let Some(ParserState::InCodeBlock { buffer, .. }) =
                        state_stack.last_mut()
                    else {
                        unreachable!();
                    };
                    buffer.push_str(&text);
                    continue;
                }
                Event::End(TagEnd::CodeBlock) => {
                    let Some(ParserState::InCodeBlock { language, buffer }) = state_stack.pop()
                    else {
                        unreachable!();
                    };
                    let highlighted_lines =
                        highlighter.highlight_code(&buffer, &language, "base16-ocean.dark");
                    blocks.push(RenderedBlock::CodeBlock {
                        language,
                        highlighted_lines,
                    });
                    continue;
                }
                // Ignore other events inside code blocks.
                _ => {
                    continue;
                }
            }
        }

        let Some(current_state) = state_stack.last() else {
            // State stack underflow — stop parsing, return what we have.
            debug_assert!(false, "parser state stack underflow");
            break;
        };

        // If we're skipping an unrecognized block, handle nesting depth.
        if let ParserState::Skipping { depth } = current_state {
            match event {
                Event::Start(_) => {
                    let new_depth = depth + 1;
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
                // Consume all other events while inside a skipped block.
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
                // Pop state first, then pop style only on the confirmed InHeading path.
                // Previously style_stack.pop() ran unconditionally before the state check,
                // which would corrupt style_stack in the error path by removing a style that
                // belonged to an active emphasis or link, not a heading.
                let level = match state_stack.pop() {
                    Some(ParserState::InHeading { level }) => {
                        style_stack.pop();
                        level
                    }
                    other => {
                        debug_assert!(
                            false,
                            "End(Heading) without InHeading state: got {other:?}"
                        );
                        1
                    }
                };
                let content = std::mem::take(&mut current_spans);
                blocks.push(RenderedBlock::Heading { level, content });
            }
            Event::End(TagEnd::Paragraph) => {
                state_stack.pop();
                let content = std::mem::take(&mut current_spans);
                blocks.push(RenderedBlock::Paragraph { content });
            }

            // ── Inline tags: passthrough (process inner text normally) ──
            // Links: ignore URL metadata, but inner Text events accumulate
            // into the current block's spans so link text remains visible.
            Event::Start(Tag::Link { .. }) => {
                style_stack.push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Event::End(TagEnd::Link) => {
                debug_assert!(!style_stack.is_empty(), "End(Link) with empty style_stack");
                style_stack.pop();
            }
            // Images: show alt text inline (no style push — images are
            // unstyled passthrough, unlike links which get ITALIC).
            Event::Start(Tag::Image { .. }) => {}
            Event::End(TagEnd::Image) => {}

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
                debug_assert!(
                    !style_stack.is_empty(),
                    "End(inline format) with empty style_stack"
                );
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

            // ── Code block start ──────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    // pulldown-cmark yields the full info string (e.g. "rust,no_run" or
                    // "python title=\"x.py\""). Take only the first whitespace-delimited
                    // token so syntect lookup and the label display get the bare language name.
                    CodeBlockKind::Fenced(lang) => {
                        // Take the bare language token, stripping both whitespace-separated
                        // attributes (GFM: "python title=\"x.py\"") and comma-separated
                        // modifiers (rustdoc: "rust,no_run", "rust,ignore").
                        lang.split_whitespace()
                            .next()
                            .unwrap_or("")
                            .split(',')
                            .next()
                            .unwrap_or("")
                            .to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                state_stack.push(ParserState::InCodeBlock {
                    language,
                    buffer: String::new(),
                });
            }

            // ── Unrecognized block-level start → skip gracefully ────
            // Block-level tags not yet rendered (lists, tables, block
            // quotes, etc.) are skipped until later phases.
            Event::Start(_) => {
                state_stack.push(ParserState::Skipping { depth: 0 });
            }

            // ── Explicitly ignored events ───────────────────────────
            // End events for tags we passthrough or skip.
            Event::End(_) => {}
            // Task list markers, footnote refs, inline HTML, etc.
            Event::TaskListMarker(_)
            | Event::FootnoteReference(_)
            | Event::InlineHtml(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_) => {}
        }
    }

    blocks
}

/// Allows `ParserState` to be used in debug_assert messages.
impl std::fmt::Debug for ParserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserState::TopLevel => write!(f, "TopLevel"),
            ParserState::InHeading { level } => write!(f, "InHeading({level})"),
            ParserState::InParagraph => write!(f, "InParagraph"),
            ParserState::InCodeBlock { language, .. } => {
                write!(f, "InCodeBlock({language})")
            }
            ParserState::Skipping { depth } => write!(f, "Skipping({depth})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;

    static TEST_HIGHLIGHTER: LazyLock<crate::highlight::Highlighter> =
        LazyLock::new(crate::highlight::Highlighter::new);

    fn h() -> &'static crate::highlight::Highlighter {
        &TEST_HIGHLIGHTER
    }

    #[test]
    fn test_parser_heading_h1_produces_heading_block() {
        let blocks = parse("# Hello", h());
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
            let blocks = parse(&md, h());
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
        let blocks = parse("Hello world", h());
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
        let blocks = parse("**bold**", h());
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
        let blocks = parse("*italic*", h());
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
        let blocks = parse("~~struck~~", h());
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
        let blocks = parse("***bold italic***", h());
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
        let blocks = parse("Use `fmt` here", h());
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
        let blocks = parse("---", h());
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], RenderedBlock::ThematicBreak));
    }

    #[test]
    fn test_parser_soft_break() {
        let blocks = parse("line one\nline two", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
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
        let blocks = parse("line one\\\nline two", h());
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
        let blocks = parse("", h());
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
        // Use a list (not code block) since code blocks are now handled.
        let md = "- item one\n- item two\n\nAfter list";
        let blocks = parse(md, h());
        assert!(blocks
            .iter()
            .any(|b| matches!(b, RenderedBlock::Paragraph { .. })));
    }

    #[test]
    fn test_parser_link_text_preserved() {
        let blocks = parse("See [the docs](https://example.com) for details", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                let all_text: String = content.iter().map(|s| s.text.as_str()).collect();
                assert!(
                    all_text.contains("the docs"),
                    "link text should be preserved, got: {all_text}"
                );
                assert!(
                    all_text.contains("See"),
                    "surrounding text preserved, got: {all_text}"
                );
                assert!(
                    all_text.contains("for details"),
                    "trailing text preserved, got: {all_text}"
                );
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_image_alt_text_preserved() {
        let blocks = parse("![alt text](image.png)", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                let all_text: String = content.iter().map(|s| s.text.as_str()).collect();
                assert!(
                    all_text.contains("alt text"),
                    "image alt text should be preserved, got: {all_text}"
                );
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_bold_inside_link() {
        let blocks = parse("[**bold link**](url)", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "bold link");
                assert!(content[0].style.add_modifier.contains(Modifier::BOLD));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    // ── Phase 2: Code block tests ───────────────────────────────

    #[test]
    fn test_parser_fenced_code_block_with_language() {
        let md = "```rust\nfn main() {}\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock {
                language,
                highlighted_lines,
            } => {
                assert_eq!(language, "rust");
                assert!(!highlighted_lines.is_empty());
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_fenced_code_block_empty_language() {
        let md = "```\nsome code\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock { language, .. } => {
                assert!(language.is_empty());
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_indented_code_block() {
        let md = "    indented code\n    more code\n";
        let blocks = parse(md, h());
        assert!(
            blocks.iter().any(|b| matches!(b, RenderedBlock::CodeBlock { .. })),
            "indented code should produce CodeBlock"
        );
    }

    #[test]
    fn test_parser_inline_code_still_styled_span() {
        let blocks = parse("Use `code` inline", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert!(content.iter().any(|s| s.text == "code"));
            }
            _ => panic!("expected Paragraph, not CodeBlock"),
        }
    }

    #[test]
    fn test_parser_code_block_content_preserved() {
        let md = "```python\ndef hello():\n    print(\"world\")\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock {
                highlighted_lines, ..
            } => {
                let all_text: String = highlighted_lines
                    .iter()
                    .flat_map(|line| line.spans.iter())
                    .map(|span| span.content.as_ref())
                    .collect();
                assert!(all_text.contains("def"), "should contain 'def'");
                assert!(all_text.contains("hello"), "should contain 'hello'");
                assert!(all_text.contains("print"), "should contain 'print'");
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_code_block_followed_by_paragraph() {
        let md = "```rust\ncode\n```\n\nAfter code";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], RenderedBlock::CodeBlock { .. }));
        assert!(matches!(&blocks[1], RenderedBlock::Paragraph { .. }));
    }

    #[test]
    fn test_parser_empty_code_block() {
        let md = "```\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock {
                highlighted_lines, ..
            } => {
                assert!(
                    highlighted_lines.is_empty(),
                    "empty code block should produce no lines"
                );
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_list_with_paragraphs_emits_no_stray_paragraphs() {
        // pulldown-cmark wraps list items in Tag::Paragraph when separated by blank lines.
        // The Skipping guard must suppress those inner paragraphs.
        let md = "- First item\n\n- Second item\n\nAfter list";
        let blocks = parse(md, h());
        let para_count = blocks
            .iter()
            .filter(|b| matches!(b, RenderedBlock::Paragraph { .. }))
            .count();
        assert_eq!(
            para_count, 1,
            "only the paragraph after the list should appear, got {para_count}"
        );
    }

    // ── Font slot strategy tests ────────────────────────────────

    #[test]
    fn test_parser_heading_h4_bold_italic() {
        let blocks = parse("#### Sub-heading", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Heading { level, content } => {
                assert_eq!(*level, 4);
                let mods = content[0].style.add_modifier;
                assert!(mods.contains(Modifier::BOLD), "h4 should have BOLD");
                assert!(mods.contains(Modifier::ITALIC), "h4 should have ITALIC");
            }
            _ => panic!("expected Heading block"),
        }
    }

    #[test]
    fn test_parser_heading_styles_distinct_modifiers() {
        let h1 = default_heading_style(1);
        let h4 = default_heading_style(4);
        // h1 has BOLD only
        assert!(h1.add_modifier.contains(Modifier::BOLD));
        assert!(!h1.add_modifier.contains(Modifier::ITALIC));
        // h4 has BOLD + ITALIC
        assert!(h4.add_modifier.contains(Modifier::BOLD));
        assert!(h4.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_parser_inline_code_has_bold_italic() {
        let style = default_code_style();
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "inline code should have BOLD"
        );
        assert!(
            style.add_modifier.contains(Modifier::ITALIC),
            "inline code should have ITALIC"
        );
    }

    #[test]
    fn test_parser_link_text_has_italic() {
        let blocks = parse("[click here](https://example.com)", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content[0].text, "click here");
                assert!(
                    content[0].style.add_modifier.contains(Modifier::ITALIC),
                    "link text should have ITALIC"
                );
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_font_slots_file_parses_without_panic() {
        let source = include_str!("../testdata/font-slots.md");
        let blocks = parse(source, h());
        assert!(blocks.len() > 20, "font-slots.md should produce many blocks");
        // Verify it contains all expected block types.
        let has_heading = blocks.iter().any(|b| matches!(b, RenderedBlock::Heading { .. }));
        let has_paragraph = blocks.iter().any(|b| matches!(b, RenderedBlock::Paragraph { .. }));
        let has_code = blocks.iter().any(|b| matches!(b, RenderedBlock::CodeBlock { .. }));
        let has_rule = blocks.iter().any(|b| matches!(b, RenderedBlock::ThematicBreak));
        assert!(has_heading, "should have headings");
        assert!(has_paragraph, "should have paragraphs");
        assert!(has_code, "should have code blocks");
        assert!(has_rule, "should have thematic breaks");
    }

    // ── Security regression tests ────────────────────────────────

    #[test]
    fn test_parser_info_string_first_word_only() {
        // pulldown-cmark yields the full info string — we must take only first word.
        // Formats like "rust,no_run", "python title=\"x.py\"" are common in docs.
        let cases = [
            ("```rust,no_run\ncode\n```", "rust"),
            ("```python title=\"x.py\"\ncode\n```", "python"),
            ("```javascript highlight=true\ncode\n```", "javascript"),
            ("```   rust   \ncode\n```", "rust"), // leading/trailing spaces trimmed by pulldown-cmark
        ];
        for (md, expected_lang) in cases {
            let blocks = parse(md, h());
            assert_eq!(blocks.len(), 1, "input: {md}");
            match &blocks[0] {
                RenderedBlock::CodeBlock { language, .. } => {
                    assert_eq!(
                        language, expected_lang,
                        "info string '{md}' should yield language '{expected_lang}', got '{language}'"
                    );
                }
                _ => panic!("expected CodeBlock for: {md}"),
            }
        }
    }
}
