# mdink — Engineering Standards

> This document defines the code quality, architecture, and engineering principles
> that apply to **every phase** of the mdink project. Every contributor (human or AI)
> must follow these standards. Each phase plan references this document.

---

## 1. Architecture Principles

### 1.1 — Pipeline Architecture (the backbone)

mdink follows a strict **unidirectional data pipeline**:

```
Markdown source
  → pulldown-cmark Parser
    → Vec<RenderedBlock>       (Intermediate Representation)
      → Layout Engine
        → PreRenderedDocument  (Vec<DocumentLine>)
          → Renderer
            → Ratatui Frame
```

**Rules:**
- Data flows in one direction only. No stage reaches back to an earlier stage.
- Each stage has a **single input type** and a **single output type**.
- Stages are connected by function calls, not by shared mutable state.
- The IR (`RenderedBlock`) is the contract between parsing and rendering — it must be
  stable and well-documented. Adding a new markdown element means adding a new enum
  variant, not changing existing ones.

### 1.2 — Module Boundaries

Each `.rs` file is a module with a **clear, single responsibility**:

| Module | Responsibility | Knows about |
|--------|---------------|-------------|
| `cli.rs` | CLI argument definition only | `clap` |
| `main.rs` | Wiring: parse args → build pipeline → run event loop | Everything (thin orchestrator) |
| `app.rs` | Application state + event dispatch | `layout`, `keybindings` |
| `parser.rs` | Markdown → IR conversion | `pulldown-cmark`, IR types |
| `layout.rs` | IR → pre-rendered document (line-level) | IR types, `textwrap` |
| `renderer.rs` | Pre-rendered document → Frame | `ratatui`, `app` (read-only) |
| `highlight.rs` | Code → highlighted lines | `syntect` |
| `images.rs` | Image loading + protocol management | `ratatui-image`, `image` |
| `theme/mod.rs` | Theme loading + style resolution | `serde`, `ratatui::style` |
| `keybindings.rs` | Key event → Action mapping | `ratatui::crossterm` |

**Rules:**
- `renderer.rs` never imports `pulldown-cmark`. It only sees `DocumentLine`.
- `parser.rs` never imports `ratatui` widgets. It only produces `RenderedBlock`.
- `highlight.rs` and `images.rs` are **leaf modules** — they don't import other mdink modules.
- `theme/mod.rs` is consumed by all rendering modules but does not depend on any of them.
- `cli.rs` has **zero dependencies** beyond `clap` — this is critical for xtask reuse.

### 1.3 — Dependency Direction

```
              cli.rs (pure clap)
                ↑
main.rs → app.rs → keybindings.rs
  │          │
  │          ↓
  │       renderer.rs
  │          │
  ↓          ↓
parser.rs  layout.rs
  │  │        │
  ↓  ↓        ↓
highlight.rs  theme/mod.rs
images.rs
```

Dependencies point **downward** (toward leaf modules). No cycles. If you find yourself
wanting module A to import module B and module B to import module A, that's a design
smell — extract the shared type into a third module (typically `types.rs` or the IR
definitions in `parser.rs`).

---

## 2. SOLID Principles (applied to Rust)

### S — Single Responsibility

Each struct, enum, and function has one reason to change.

- `App` manages **state** (scroll position, mode). It does not render.
- `renderer::draw()` **renders**. It does not modify state.
- `parser::parse()` **converts** markdown events to IR. It does not style or lay out.

**Litmus test:** Can you describe what this module does in one sentence without "and"?

### O — Open/Closed

The `RenderedBlock` enum is the extensibility point. Adding a new block type
(e.g., `MathBlock`) means:
1. Add a variant to `RenderedBlock`
2. Handle it in `layout.rs` (new arm in match)
3. Handle it in `renderer.rs` (new arm in match)

No existing code needs modification — only new match arms. Use `#[non_exhaustive]`
on public enums if exposing as a library.

### L — Liskov Substitution

Rust doesn't have inheritance, but the principle applies to trait implementations:
- Any `DocumentLine` variant must be renderable by the renderer without special-casing
  beyond a match arm.
- Any theme JSON that validates against the schema must produce correct output.

### I — Interface Segregation

Functions accept the **narrowest type** they need:

```rust
// Good: takes only what it needs
fn flatten(blocks: &[RenderedBlock], width: u16, theme: &DocumentStyle) -> PreRenderedDocument

// Bad: takes the entire App when it only needs width
fn flatten(app: &App) -> PreRenderedDocument
```

Prefer passing `&MarkdownTheme` subsections (e.g., `&HeadingStyle`) over the entire theme
when a function only needs heading styles.

### D — Dependency Inversion

High-level modules should not depend on low-level details:

- The parser doesn't know which syntax highlighter is used — it receives a
  `&Highlighter` (our own struct, not a raw `syntect` type).
- The renderer doesn't know how images are loaded — it receives a `&mut StatefulProtocol`
  via index.
- Theme resolution is behind `load_theme()` — callers don't know if it came from
  embedded JSON, a file, or an env var.

---

## 3. Design Patterns

### 3.1 — State Machine (parser)

The pulldown-cmark event stream is processed with an explicit state machine:

```rust
enum ParserState {
    TopLevel,
    InHeading { level: u8 },
    InParagraph,
    InCodeBlock { language: String, buffer: String },
    InList { depth: u8, ordered: bool, counter: u64 },
    InBlockQuote { depth: u8 },
    InTable { phase: TablePhase },
    // ...
}
```

**Rules:**
- Every `Start(Tag)` event transitions to a new state. Every `End(TagEnd)` returns to the previous state.
- Use a **state stack** (`Vec<ParserState>`) for nesting (lists inside quotes, etc.).
- Never use boolean flags like `in_heading: bool` — they don't compose. Use the enum.
- All `Event::Text` content is accumulated into the current state's buffer.

### 3.2 — Builder Pattern (theme style construction)

Converting theme JSON fields to `ratatui::Style` uses a builder chain:

```rust
Style::default()
    .fg(parse_color(&heading.fg)?)
    .bg(parse_color(&heading.bg)?)
    .add_modifier(if heading.bold { Modifier::BOLD } else { Modifier::empty() })
```

This is idiomatic Ratatui and should be used consistently everywhere styles are constructed.

### 3.3 — Strategy Pattern (image protocol)

`ratatui-image`'s `Picker` selects the right terminal protocol at runtime (Sixel, Kitty,
iTerm2, halfblocks). Our code doesn't branch on protocol — `StatefulProtocol` is the
unified interface. This is the Strategy pattern: the algorithm (encoding) varies, but the
calling code is identical.

### 3.4 — Index-based Indirection (images)

`StatefulProtocol` is `!Clone` and requires `&mut` at render time. To avoid borrow-checker
conflicts when iterating `DocumentLine`s while rendering images:

```rust
// DocumentLine stores an index, not the protocol itself
ImageStart { protocol_index: usize, height: u16 }

// App owns the actual protocols
struct App {
    image_protocols: Vec<StatefulProtocol>,
}
```

This is a common Rust pattern for arena-like ownership. Use it whenever you have
non-cloneable resources referenced from multiple places.

### 3.5 — Visitor Pattern (layout flattening)

`layout::flatten()` visits each `RenderedBlock` variant and emits `DocumentLine`s.
This is a simple match-based visitor. If the number of block types grows large (>12),
consider extracting a `BlockVisitor` trait, but for now `match` is cleaner.

---

## 4. Error Handling

### 4.1 — Philosophy

**Fail gracefully, never crash on user input.**

- mdink is a **viewer** — it should display *something* for any input, even malformed markdown.
- Errors fall into two categories:
  - **Fatal:** Can't open file, can't initialize terminal → print error and exit with non-zero code
  - **Recoverable:** Bad image, unknown language, malformed theme → degrade gracefully with fallback

### 4.2 — Error Types

Use `color-eyre` for the top-level `Result<()>` in `main()`:

```rust
fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    // ...
}
```

Within library modules, use domain-specific error types:

```rust
// Good: specific, actionable
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme not found: {name}")]
    NotFound { name: String },
    #[error("invalid color string: {value}")]
    InvalidColor { value: String },
    #[error("failed to parse theme JSON: {source}")]
    ParseError { #[from] source: serde_json::Error },
}

// Bad: stringly-typed
fn load_theme(name: &str) -> Result<Theme, String>
```

### 4.3 — Graceful Degradation Rules

| Failure | Degraded behavior |
|---------|-------------------|
| Image file not found | Display `[image: alt_text]` |
| Unknown code language | Render as plain text (no highlighting) |
| Theme file invalid | Fall back to built-in dark theme + warn on stderr |
| Terminal doesn't support images | Skip images, show alt text |
| Unicode character can't be measured | Assume width 1 |
| Markdown has unclosed tags | `pulldown-cmark` handles this — just render what it emits |

### 4.4 — Panic Policy

- **Never panic on user-controlled input.** No `unwrap()` on file reads, parse results,
  or user-provided values.
- `unwrap()` is acceptable **only** on invariants that are guaranteed by the program
  structure (e.g., an index known to be in-bounds from a preceding check). Add a comment
  explaining the invariant:

```rust
// SAFETY: index was returned by load_image(), which pushes to this vec
let protocol = &mut self.protocols[index];
```

- Prefer `expect("reason")` over bare `unwrap()` when the invariant isn't obvious.

---

## 5. Testing Strategy

### 5.1 — Test Pyramid

```
           ┌──────────┐
           │  Visual   │   Snapshot tests with TestBackend (few)
          ┌┴──────────┴┐
          │ Integration │   Render full .md files without panic (some)
        ┌─┴────────────┴─┐
        │   Unit Tests    │   Per-function, per-module (many)
        └────────────────┘
```

### 5.2 — Unit Tests (required for every module)

Every module with logic must have a `#[cfg(test)] mod tests` block.

| Module | What to test |
|--------|-------------|
| `parser.rs` | Known markdown string → expected `Vec<RenderedBlock>` variants |
| `layout.rs` | Known blocks → expected line count, wrapping behavior |
| `highlight.rs` | Known code + language → non-empty highlighted lines |
| `theme/mod.rs` | Color string → `Color`, JSON → `MarkdownTheme`, default themes valid |
| `keybindings.rs` | `KeyEvent` → expected `Action` |
| `app.rs` | Scroll bounds, visible range calculation |

**Test naming convention:** `test_{module}_{scenario}_{expected_outcome}`
```rust
#[test]
fn test_parser_heading_h1_produces_heading_block() { ... }

#[test]
fn test_layout_long_paragraph_wraps_at_width() { ... }

#[test]
fn test_theme_parse_color_hex_valid() { ... }
```

### 5.3 — Integration Tests

Place in `tests/` directory (cargo's default integration test location):

```rust
// tests/render_testdata.rs
#[test]
fn render_basic_md_no_panic() {
    let source = std::fs::read_to_string("testdata/basic.md").unwrap();
    let blocks = parser::parse(&source);
    let doc = layout::flatten(&blocks, 80);
    assert!(doc.total_height > 0);
}
```

Each `testdata/*.md` file gets a no-panic integration test.

### 5.4 — Visual/Snapshot Tests (Phase 6+)

Use `ratatui::backend::TestBackend` to capture rendered output:

```rust
let backend = TestBackend::new(80, 24);
let mut terminal = Terminal::new(backend)?;
terminal.draw(|f| renderer::draw(f, &app))?;
let buffer = terminal.backend().buffer().clone();
// Compare buffer content against expected snapshot
```

### 5.5 — Test Data Files

All test markdown files live in `testdata/`. Each file exercises a specific feature:

| File | Exercises | Introduced in |
|------|-----------|---------------|
| `basic.md` | Headings, paragraphs, inline styles, horizontal rules | Phase 1 |
| `code-blocks.md` | Fenced (rust, python, js, unknown), indented code | Phase 2 |
| `lists.md` | Ordered, unordered, nested, task lists | Phase 3 |
| `blockquotes.md` | Single, nested, with inner formatting | Phase 3 |
| `tables.md` | Aligned columns, varying widths | Phase 3 |
| `images.md` | Local image refs, missing image | Phase 4 |
| `full-featured.md` | Every supported element combined | Phase 6 |

### 5.6 — CI Enforcement

Every push/PR must pass:
- `cargo test --locked`
- `cargo clippy --locked -- -D warnings`
- `cargo fmt -- --check` (if rustfmt is configured)

No exceptions. Tests are not optional.

---

## 6. Code Quality

### 6.1 — Naming Conventions

Follow Rust's standard conventions:
- Types: `PascalCase` — `RenderedBlock`, `PreRenderedDocument`, `MarkdownTheme`
- Functions/methods: `snake_case` — `parse_color`, `load_theme`, `scroll_down`
- Constants: `SCREAMING_SNAKE_CASE` — `DEFAULT_THEME_NAME`
- Enum variants: `PascalCase` — `DocumentLine::ImageStart`
- Module files: `snake_case.rs` — `highlight.rs`, `keybindings.rs`

**Domain-specific conventions:**
- IR types are named after what they represent, not how they're built: `RenderedBlock` not `ParseResult`
- Layout types describe the output format: `DocumentLine`, `PreRenderedDocument`
- Functions that convert between types use `to_`/`from_`/`into_` prefixes

### 6.2 — Documentation

- **Public types and functions** get a `///` doc comment explaining *what* they do
  and *why*, not *how* (the code shows how).
- **Module-level** `//!` doc comments explain the module's role in the pipeline.
- **No doc comments** on private helpers unless the logic is non-obvious.
- Doc comments on enum variants when the variant name alone is ambiguous.

```rust
//! Markdown parser: converts pulldown-cmark events into the RenderedBlock IR.
//!
//! This module is the first stage of the rendering pipeline.

/// A rendered markdown block ready for layout.
///
/// Each variant corresponds to a markdown block-level element.
/// Inline styling is carried via `Vec<StyledSpan>` in content fields.
pub enum RenderedBlock {
    /// Heading with level (1–6). Content carries inline styles.
    Heading { level: u8, content: Vec<StyledSpan> },
    // ...
}
```

### 6.3 — Clippy and Formatting

- **All clippy warnings are errors** in CI (`-D warnings`).
- Run `cargo clippy` locally before committing.
- Use `#[allow(clippy::...)]` only with a comment explaining why.
- Prefer `rustfmt` defaults. Don't bikeshed formatting.

### 6.4 — Import Organization

Group imports in this order (separated by blank lines):

```rust
// 1. Standard library
use std::collections::HashMap;
use std::path::PathBuf;

// 2. External crates
use pulldown_cmark::{Event, Parser, Tag};
use ratatui::style::{Color, Modifier, Style};

// 3. Internal modules (crate::)
use crate::parser::RenderedBlock;
use crate::theme::MarkdownTheme;
```

### 6.5 — Avoid Premature Abstraction

- Three similar lines of code is better than a premature `trait`.
- Don't create a `Renderable` trait until at least 3 types implement it.
- Don't create a `util.rs` dumping ground. If a helper is used in one module, keep it there.
- Don't add `pub` to anything that doesn't need to be public.

### 6.6 — Type Safety Over Runtime Checks

Prefer encoding invariants in the type system:

```rust
// Good: heading level is bounded at the type level
pub struct HeadingLevel(u8);  // with constructor that validates 1..=6

// Acceptable: simple validation in the parser
Heading { level: u8, ... }   // pulldown-cmark guarantees 1..=6

// Bad: stringly-typed
Heading { level: String, ... }
```

Use newtypes when a primitive value has domain meaning (e.g., `ScrollOffset(usize)`,
`TerminalWidth(u16)`) — but only when it prevents real bugs. Don't over-engineer.

---

## 7. Robustness

### 7.1 — Input Tolerance

mdink must handle:
- Empty files (render blank screen)
- Binary files (display error, don't crash)
- Extremely large files (stream-parse if needed; pre-rendering may OOM — set a reasonable limit)
- Files with every Unicode block (CJK, RTL, emoji, combining characters)
- Markdown with deeply nested structures (block quotes in lists in block quotes)
- Pathological markdown (10,000 headings, tables with 500 columns)

### 7.2 — Terminal Tolerance

mdink must handle:
- Terminals as small as 40×10 (truncate, don't panic)
- Terminal resize mid-display (re-render on `SIGWINCH` / resize event)
- No color support (graceful degradation to unstyled text)
- No image support (alt text fallback)
- Piped output (detect `!isatty()` → pager mode or error)

### 7.3 — Resource Safety

- **Terminal restoration:** Always call `ratatui::restore()` before exit, including on panic.
  Use a panic hook:
  ```rust
  let original_hook = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
      let _ = ratatui::restore();
      original_hook(info);
  }));
  ```
- **No resource leaks:** Image decoding allocates — drop images when scrolling past them
  (or keep them cached, but bound the cache size).
- **Syntect sets are expensive:** Load once, share via `&Highlighter`. Never clone them.

---

## 8. Coherency and Consistency

### 8.1 — Consistent Error Messages

All user-facing error messages follow the pattern:
```
mdink: {action failed}: {specific reason}
```

Examples:
```
mdink: could not open file: testdata/missing.md (No such file or directory)
mdink: invalid theme: unknown color "purplish" in heading.fg
mdink: image load failed: photo.png (unsupported format)
```

### 8.2 — Consistent Style Application

All styling goes through the theme system (after Phase 5). There must be **zero hardcoded
colors** in the renderer after theming is implemented. The renderer reads from
`&MarkdownTheme`, never constructs `Color::Rgb(...)` directly.

Before Phase 5, hardcoded styles are allowed but must be **centralized** in one location
per module (a `fn default_styles()` or equivalent), never scattered through logic.

### 8.3 — Consistent Match Exhaustiveness

Every `match` on `RenderedBlock` or `DocumentLine` must cover all variants explicitly.
Never use a catch-all `_ =>` on these enums — when a new variant is added in a later phase,
the compiler must force you to handle it everywhere.

```rust
// Good: compiler catches missing variants
match block {
    RenderedBlock::Heading { .. } => { ... }
    RenderedBlock::Paragraph { .. } => { ... }
    RenderedBlock::CodeBlock { .. } => { ... }
    RenderedBlock::ThematicBreak => { ... }
    // compiler error when new variant added ← this is what we want
}

// Bad: silently ignores new variants
match block {
    RenderedBlock::Heading { .. } => { ... }
    _ => {}
}
```

### 8.4 — Consistent Function Signatures

Functions in the pipeline follow predictable signatures:

```rust
// Parser: always returns Vec<RenderedBlock>
pub fn parse(source: &str, ...) -> Vec<RenderedBlock>

// Layout: always returns PreRenderedDocument
pub fn flatten(blocks: &[RenderedBlock], width: u16, ...) -> PreRenderedDocument

// Renderer: always takes Frame + App
pub fn draw(frame: &mut Frame, app: &App)
// (or &mut App when mutable access is needed for images)
```

Don't vary these signatures without good reason. Consistency makes the pipeline
predictable and navigable.

---

## 9. Performance Considerations

### 9.1 — Pre-rendering Budget

Pre-rendering happens once (on file load or terminal resize). It should complete in <100ms
for typical files (<10,000 lines). Measure if you suspect a bottleneck.

### 9.2 — Render Budget

Each frame render (called on every keypress and resize) must complete in <16ms (60fps).
Since we only render visible lines (viewport-sized), this should be trivially fast.
Never iterate the entire document during rendering.

### 9.3 — Allocation Strategy

- Pre-rendering allocates freely (it runs once).
- Frame rendering should minimize allocations — reuse buffers where possible.
- The `'static` lifetime on `Line<'static>` and `Span<'static>` means lines own their
  data. This is intentional: it avoids lifetime gymnastics and the allocation cost is
  paid once during pre-rendering, not per-frame.

---

## 10. Phase Gate Checklist

Before marking any phase as complete, verify:

- [ ] **Compiles:** `cargo build` clean, no warnings
- [ ] **Lints:** `cargo clippy -- -D warnings` clean
- [ ] **Tests:** `cargo test` all passing, new tests for new code
- [ ] **No regressions:** All previous phase features still work
- [ ] **Architecture:** New code follows module boundaries (section 1.2)
- [ ] **Error handling:** No new `unwrap()` on user input (section 4.4)
- [ ] **Match exhaustiveness:** No new `_ =>` catch-all on IR enums (section 8.3)
- [ ] **Documentation:** Public types/functions have doc comments
- [ ] **Test data:** New features have corresponding `testdata/*.md` files
- [ ] **CI green:** Push passes the CI workflow
