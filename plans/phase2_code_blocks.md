# Phase 2: Code Blocks with Syntax Highlighting

> **Prerequisites:** Phase 1 complete
> **Standards:** All code must follow [standards.md](standards.md)
> **New dependencies:** `syntect = "5.2"`

**Goal:** Render fenced code blocks with syntect-powered syntax highlighting, language
labels, and optional line numbers.

---

## 2.1 — Highlight Module (`src/highlight.rs`)

A **leaf module** — no imports from other mdink modules (see [standards.md §1.2](standards.md)).

### `Highlighter` struct

```rust
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}
```

- `SyntaxSet::load_defaults_newlines()` and `ThemeSet::load_defaults()` are expensive
  (~50ms each). Load **once** at startup, store in `Highlighter`.
  (See [standards.md §7.3](standards.md) — Resource Safety)
- Never clone these sets.

### Methods

```rust
impl Highlighter {
    pub fn new() -> Self
    pub fn highlight_code(&self, code: &str, language: &str, theme_name: &str) -> Vec<Line<'static>>
}
```

- `highlight_code`: Look up syntax by language token via `syntax_set.find_syntax_by_token(language)`.
  Fall back to plain text if not found. Use `HighlightLines` to highlight line-by-line.
  Convert each syntect `(Style, &str)` pair via the bridge function below.

### syntect → ratatui bridge (replaces syntect-tui crate)

```rust
/// Convert a syntect highlighted segment into a ratatui Span.
/// Returns `Span<'static>` because `text.to_string()` creates owned data.
fn syntect_style_to_span(text: &str, style: SyntectStyle) -> Span<'static> {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    let bg = Color::Rgb(style.background.r, style.background.g, style.background.b);
    let mut modifier = Modifier::empty();
    if style.font_style.contains(FontStyle::BOLD) { modifier |= Modifier::BOLD; }
    if style.font_style.contains(FontStyle::ITALIC) { modifier |= Modifier::ITALIC; }
    if style.font_style.contains(FontStyle::UNDERLINE) { modifier |= Modifier::UNDERLINED; }
    Span::styled(text.to_string(), Style::default().fg(fg).bg(bg).add_modifier(modifier))
}
```

**Standards note:** This module wraps `syntect` behind our own `Highlighter` type.
The rest of the codebase never imports `syntect` directly — Dependency Inversion.
(See [standards.md §2 — SOLID/D](standards.md))

---

## 2.2 — Parser Extension

### New IR variant

Add to `RenderedBlock`:
```rust
CodeBlock {
    language: String,
    highlighted_lines: Vec<Line<'static>>,
}
```

### Parser state machine extension

New state:
```rust
ParserState::InCodeBlock { language: String, buffer: String }
```

Events to handle:
- `Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))` → transition to `InCodeBlock`, save language
- `Event::Start(Tag::CodeBlock(CodeBlockKind::Indented))` → transition to `InCodeBlock`, language = `""`
- `Event::Text(content)` while in `InCodeBlock` → append to buffer
- `Event::End(TagEnd::CodeBlock)` → pass buffer to `highlighter.highlight_code()`, push `CodeBlock`, return to `TopLevel`

### Signature change

```rust
// Phase 1:
pub fn parse(source: &str) -> Vec<RenderedBlock>

// Phase 2:
pub fn parse(source: &str, highlighter: &Highlighter) -> Vec<RenderedBlock>
```

**Standards note:** The parser receives `&Highlighter` (our type), not raw syntect types.
This keeps the module boundary clean. (See [standards.md §2 — SOLID/D](standards.md))

---

## 2.3 — Layout Extension

### New `DocumentLine` variant

```rust
DocumentLine::Code(Line<'static>)
```

### Flattening `CodeBlock`

For each `RenderedBlock::CodeBlock { language, highlighted_lines }`:

1. *(Optional)* Emit a header line: language label right-aligned on a background bar
2. For each highlighted line → emit `DocumentLine::Code(line)`
3. *(Optional)* Prefix each line with line number (hardcoded on/off for now; theme-controlled in Phase 5)
4. *(Optional)* Emit top/bottom border lines
5. Add inter-block spacing

**Standards note:** Add `Code` to the `DocumentLine` match in `layout.rs` and `renderer.rs`.
No `_ =>` catch-all — compiler must flag every match site.
(See [standards.md §8.3](standards.md))

---

## 2.4 — Renderer Extension

For `DocumentLine::Code(line)`:
- Render with a distinct background color (hardcoded dark gray for Phase 2)
- The highlighted spans already carry foreground colors from syntect
- Apply padding on left/right for visual separation from surrounding text

---

## 2.5 — Test Data and Tests

### Test data

Create `testdata/code-blocks.md`:
- Fenced block with `rust`
- Fenced block with `python`
- Fenced block with `javascript`
- Fenced block with unknown/empty language
- Indented code block
- Code block with very long lines (test no-wrap behavior)
- Empty code block

### Unit tests

**`highlight.rs`:**
- Known Rust code → non-empty `Vec<Line>` with colored spans
- Unknown language → plain text output (no crash)
- Empty code string → empty or single-empty-line output

**`parser.rs`:**
- Fenced block → `CodeBlock` variant with correct language
- Indented block → `CodeBlock` variant with empty language
- Inline code (`` `backtick` ``) → still produces `StyledSpan` (not `CodeBlock`)

---

## Phase 2 — Definition of Done

- [ ] Fenced code blocks render with syntax highlighting
- [ ] Language auto-detection from fence info string works
- [ ] Unknown languages fall back to plain text (no crash)
- [ ] Indented code blocks render as code
- [ ] Code blocks have a visually distinct background
- [ ] Line numbers display (optional, can be hardcoded on)
- [ ] Language label shows in the code block header
- [ ] `Highlighter` loads syntax/theme sets once (no per-block reloading)
- [ ] No direct `syntect` imports outside `highlight.rs`
- [ ] All `match` arms updated for new `CodeBlock` / `Code` variants
- [ ] `cargo test` passes with highlight and parser tests
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Phase 1 features still work (no regressions)
- [ ] Phase gate checklist from [standards.md §10](standards.md) passes

**Files created/modified:**
- Created: `src/highlight.rs`, `testdata/code-blocks.md`
- Modified: `Cargo.toml` (uncomment syntect), `src/parser.rs`, `src/layout.rs`, `src/renderer.rs`, `src/main.rs`
