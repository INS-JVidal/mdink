# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo test                          # run all tests
cargo test test_name                # run a single test by name (substring match)
cargo clippy -- -D warnings         # lint (must be clean before committing)
cargo run -- testdata/basic.md      # run the app (requires a real terminal)
cargo run -- testdata/font-slots.md # exercise all four font-slot rendering paths
```

## Architecture

mdink is a terminal markdown renderer (ratatui + pulldown-cmark + syntect). It uses a strict **unidirectional pipeline** — each stage is a pure function over the previous stage's output:

```
&str (markdown source)
  → parser::parse()      → Vec<RenderedBlock>         (semantic IR)
    → layout::flatten()  → PreRenderedDocument         (layout-resolved lines)
      → renderer::draw() → ratatui Frame               (pixels)
```

No stage imports from a later stage. The `Highlighter` from `highlight.rs` is passed into `parse()` as a parameter — it never touches layout or renderer.

### Module responsibilities

| Module | Input | Output | Key type |
|--------|-------|--------|----------|
| `parser.rs` | `&str` + `&Highlighter` | semantic blocks | `RenderedBlock` |
| `highlight.rs` | `&str` (code) + language + theme | colored spans | `Vec<Line<'static>>` |
| `layout.rs` | `&[RenderedBlock]` + width | display-ready lines | `PreRenderedDocument` |
| `renderer.rs` | `&App` | writes to frame | — |
| `app.rs` | keyboard events | scroll state mutation | `App` |

### `RenderedBlock` — the IR

```rust
pub enum RenderedBlock {
    Heading { level: u8, content: Vec<StyledSpan> },
    Paragraph { content: Vec<StyledSpan> },
    CodeBlock { language: String, highlighted_lines: Vec<Line<'static>> },
    ThematicBreak,
    Spacer { lines: u16 },
}
```

`StyledSpan` carries owned `text: String` + `style: Style`. Adding a new block type means adding a variant here, a match arm in `parser.rs`, a match arm in `layout.rs`, and a match arm in `renderer.rs` — no other files need changing.

### Layout word-wrap algorithm

`layout.rs` cannot use ratatui's built-in wrapping because ratatui has no way to propagate per-character styles across line breaks. Instead:

1. Build a plain `String` from all spans + a parallel `Vec<Style>` indexed by byte offset.
2. Call `textwrap::wrap()` on the plain string.
3. Walk each wrapped line with a byte cursor; look up each character's style from the byte map; reconstruct `Span`s.

Hard breaks (`\n` embedded in `StyledSpan.text`) are handled by splitting the span and wrapping each half independently before merging.

### Font slot strategy

Modern terminals (WezTerm, Kitty, Alacritty) allow a different font per ANSI modifier combo. mdink deliberately maps markdown elements to slots:

| Slot | Modifier | Elements |
|------|----------|----------|
| Normal | none | body text |
| Bold | `BOLD` | h1–h3, `**strong**` |
| Italic | `ITALIC` | `*emphasis*`, links |
| Bold+Italic | `BOLD\|ITALIC` | h4–h6, `` `inline code` `` |

Code block comments are forced to `ITALIC` via a color-matching heuristic: `resolve_comment_color()` reads the `comment` scope's color from the syntect theme once, then any token whose foreground matches that color gets `ITALIC` added.

### Invariants to preserve

- **Highlight size guard:** `highlight.rs` rejects code blocks > 512 KB (Oniguruma can OOM on large inputs).
- **File size guard:** `main.rs` rejects files > 100 MB before terminal init.
- **Width clamp:** `layout.rs` clamps width to ≥ 1; `textwrap` has undefined behavior at width 0.
- **Style stack:** `parser.rs` pushes a `Style` for each inline format open tag and pops it on the matching close tag. All pop sites have `debug_assert!(!style_stack.is_empty())`.
- **Terminal restore:** `TERMINAL_ACTIVE` flag in `main.rs` ensures the panic hook only restores the terminal if it was successfully initialized. Never remove this flag.
- **Leaf module:** `highlight.rs` never imports from other mdink modules. syntect types must not leak into parser, layout, or renderer.

### Resize handling

On terminal resize, `main.rs` re-calls `layout::flatten(&blocks, new_width)` and stores the new `PreRenderedDocument` in `App`. `blocks` (the `Vec<RenderedBlock>`) is kept alive in `main.rs` for this purpose. Layout is stateless and idempotent — calling it again is always safe.

## Planned phases

The roadmap is tracked in `plans/overview.md`. Phase 3 adds lists/tables/blockquotes (new `RenderedBlock` variants). Phase 5 adds theming — the most invasive change, threading a `&Theme` through all style-producing functions. New work should avoid hardcoding colors or styles that will need to be theme-configurable.
