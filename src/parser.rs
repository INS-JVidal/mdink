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

// ── ParseContext ─────────────────────────────────────────────────────────────

/// Accumulates all mutable parser state across a single `parse()` call.
///
/// Exists solely as an implementation detail of `parse()` — it is created
/// in `ParseContext::new`, driven by `process()`, and consumed to return
/// the final `Vec<RenderedBlock>`. Not part of the public API.
struct ParseContext<'a> {
    highlighter: &'a crate::highlight::Highlighter,
    blocks: Vec<RenderedBlock>,
    /// Block-level state machine (never empty while parsing).
    state_stack: Vec<ParserState>,
    /// Inline formatting modifier stack (push on Start, pop on End).
    style_stack: Vec<Style>,
    /// Spans accumulated for the block currently being built.
    current_spans: Vec<StyledSpan>,
}

impl<'a> ParseContext<'a> {
    fn new(highlighter: &'a crate::highlight::Highlighter) -> Self {
        Self {
            highlighter,
            blocks: Vec::new(),
            state_stack: vec![ParserState::TopLevel],
            style_stack: Vec::new(),
            current_spans: Vec::new(),
        }
    }

    /// Drives the pulldown-cmark event stream and returns the finished blocks.
    fn process(mut self, source: &str) -> Vec<RenderedBlock> {
        let options =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;

        for event in Parser::new_ext(source, options) {
            if self.state_stack.is_empty() {
                // State stack underflow — parser invariant violated. Stop here
                // rather than panic so partially-parsed output is still returned.
                debug_assert!(false, "parser state stack underflow");
                break;
            }
            self.on_event(event);
        }

        self.blocks
    }

    // ── Event routing ────────────────────────────────────────────────────────

    /// Routes each event to the appropriate handler based on current state.
    fn on_event(&mut self, event: Event) {
        if matches!(self.state_stack.last(), Some(ParserState::InCodeBlock { .. })) {
            self.on_code_block_event(event);
        } else if matches!(self.state_stack.last(), Some(ParserState::Skipping { .. })) {
            self.on_skipping_event(event);
        } else {
            self.dispatch(event);
        }
    }

    /// Handles events when inside a fenced/indented code block.
    ///
    /// Accumulates text into the buffer; on `End(CodeBlock)` runs syntax
    /// highlighting and emits the finished `CodeBlock` block.
    fn on_code_block_event(&mut self, event: Event) {
        match event {
            Event::Text(text) => {
                if let Some(ParserState::InCodeBlock { buffer, .. }) =
                    self.state_stack.last_mut()
                {
                    buffer.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(ParserState::InCodeBlock { language, buffer }) =
                    self.state_stack.pop()
                {
                    let highlighted_lines = self.highlighter.highlight_code(
                        &buffer,
                        &language,
                        "base16-ocean.dark",
                    );
                    self.blocks
                        .push(RenderedBlock::CodeBlock { language, highlighted_lines });
                }
            }
            // Ignore all other events (syntax, meta) inside a code block.
            _ => {}
        }
    }

    /// Handles events when inside an unrecognized block being skipped.
    ///
    /// Tracks nesting depth via `Skipping { depth }` so that nested
    /// unrecognized blocks don't prematurely end the skip.
    fn on_skipping_event(&mut self, event: Event) {
        // Copy depth out to release the shared borrow before mutation below.
        let depth = match self.state_stack.last() {
            Some(ParserState::Skipping { depth }) => *depth,
            _ => return,
        };
        match event {
            Event::Start(_) => {
                self.state_stack.pop();
                self.state_stack.push(ParserState::Skipping { depth: depth + 1 });
            }
            Event::End(_) if depth == 0 => {
                self.state_stack.pop();
            }
            Event::End(_) => {
                self.state_stack.pop();
                self.state_stack.push(ParserState::Skipping { depth: depth - 1 });
            }
            _ => {}
        }
    }

    /// Dispatches normal (non-code-block, non-skipping) events.
    fn dispatch(&mut self, event: Event) {
        match event {
            // ── Block-level start ────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => self.start_heading(level),
            Event::Start(Tag::Paragraph) => self.start_paragraph(),
            Event::Start(Tag::CodeBlock(kind)) => self.start_code_block(kind),

            // ── Inline passthrough ───────────────────────────────────
            // Links: render text in the italic font slot; URL is ignored.
            Event::Start(Tag::Link { .. }) => {
                self.push_style(Style::default().add_modifier(Modifier::ITALIC));
            }
            // Images: show alt text unstyled (no style push).
            Event::Start(Tag::Image { .. }) => {}

            // ── Inline formatting ────────────────────────────────────
            Event::Start(Tag::Emphasis) => {
                self.push_style(Style::default().add_modifier(Modifier::ITALIC));
            }
            Event::Start(Tag::Strong) => {
                self.push_style(Style::default().add_modifier(Modifier::BOLD));
            }
            Event::Start(Tag::Strikethrough) => {
                self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }

            // Any unrecognized block tag — skip until its matching End.
            // MUST be last among Start arms so it doesn't shadow specific variants above.
            Event::Start(_) => self.state_stack.push(ParserState::Skipping { depth: 0 }),

            // ── Block-level end ──────────────────────────────────────
            Event::End(TagEnd::Heading(_)) => self.end_heading(),
            Event::End(TagEnd::Paragraph) => self.end_paragraph(),

            // ── Inline end ───────────────────────────────────────────
            Event::End(TagEnd::Link) => self.pop_style(),
            Event::End(TagEnd::Image) => {}
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                self.pop_style();
            }

            // ── Text content ─────────────────────────────────────────
            Event::Text(text) => self.push_text(&text),
            Event::Code(text) => self.push_inline_code(&text),
            Event::SoftBreak => self.push_soft_break(),
            Event::HardBreak => self.push_hard_break(),
            Event::Rule => self.blocks.push(RenderedBlock::ThematicBreak),

            // ── Ignored ──────────────────────────────────────────────
            // End events for passthrough/skipped tags have no handler.
            Event::End(_) => {}
            Event::TaskListMarker(_)
            | Event::FootnoteReference(_)
            | Event::InlineHtml(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_) => {}
        }
    }

    // ── Block handlers ───────────────────────────────────────────────────────

    fn start_heading(&mut self, level: HeadingLevel) {
        let lvl = heading_level_to_u8(level);
        self.style_stack.push(default_heading_style(lvl));
        self.current_spans.clear();
        self.state_stack.push(ParserState::InHeading { level: lvl });
    }

    fn end_heading(&mut self) {
        // Pop state first; only pop style if state confirms we were in a heading.
        // This prevents corrupting the style stack on malformed event sequences.
        let level = match self.state_stack.pop() {
            Some(ParserState::InHeading { level }) => {
                self.style_stack.pop();
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
        let content = std::mem::take(&mut self.current_spans);
        self.blocks.push(RenderedBlock::Heading { level, content });
    }

    fn start_paragraph(&mut self) {
        self.current_spans.clear();
        self.state_stack.push(ParserState::InParagraph);
    }

    fn end_paragraph(&mut self) {
        self.state_stack.pop();
        let content = std::mem::take(&mut self.current_spans);
        self.blocks.push(RenderedBlock::Paragraph { content });
    }

    fn start_code_block(&mut self, kind: CodeBlockKind) {
        let language = match kind {
            // pulldown-cmark yields the full info string (e.g. "rust,no_run" or
            // "python title=\"x.py\""). Take only the first whitespace-delimited
            // token so syntect lookup and the label display get the bare language name.
            CodeBlockKind::Fenced(lang) => lang
                .split_whitespace()
                .next()
                .unwrap_or("")
                .split(',')
                .next()
                .unwrap_or("")
                .to_string(),
            CodeBlockKind::Indented => String::new(),
        };
        self.state_stack
            .push(ParserState::InCodeBlock { language, buffer: String::new() });
    }

    // ── Style stack helpers ──────────────────────────────────────────────────

    fn push_style(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        debug_assert!(!self.style_stack.is_empty(), "pop_style on empty style_stack");
        self.style_stack.pop();
    }

    // ── Span builders ────────────────────────────────────────────────────────

    fn push_text(&mut self, text: &str) {
        let style = effective_style(&self.style_stack);
        self.current_spans.push(StyledSpan { text: text.to_string(), style });
    }

    fn push_inline_code(&mut self, text: &str) {
        self.current_spans
            .push(StyledSpan { text: text.to_string(), style: default_code_style() });
    }

    fn push_soft_break(&mut self) {
        let style = effective_style(&self.style_stack);
        self.current_spans.push(StyledSpan { text: " ".to_string(), style });
    }

    fn push_hard_break(&mut self) {
        let style = effective_style(&self.style_stack);
        self.current_spans.push(StyledSpan { text: "\n".to_string(), style });
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Parses a markdown source string into the RenderedBlock IR.
///
/// Enables GFM extensions (strikethrough, tables, tasklists) so that
/// user markdown containing these features doesn't break — even though
/// tables and lists aren't rendered until later phases.
pub fn parse(source: &str, highlighter: &crate::highlight::Highlighter) -> Vec<RenderedBlock> {
    ParseContext::new(highlighter).process(source)
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
#[path = "parser_tests.rs"]
mod tests;
