# Phase 6: Links, Footnotes, and Polish

> **Prerequisites:** Phase 5 complete
> **Standards:** All code must follow [standards.md](standards.md)
> **New dependencies:** *(optional: `ureq = "3"` for URL fetching)*

**Goal:** Handle remaining markdown elements, add navigation features (search, heading
jump), support multiple input sources, and polish the UX.

---

## 6.1 — Links

### Parser

- Handle `Event::Start(Tag::Link { dest_url, title, .. })` and `Event::End(TagEnd::Link)`
- Store link URL alongside styled text in `StyledSpan` (add an optional `url: Option<String>` field, or use a separate `LinkSpan` type)
- Build a link index in `PreRenderedDocument`: `Vec<(usize, String, String)>` (line_index, text, url)

### Renderer

- Render link text with underline + link color from `theme.link`
- **OSC 8 hyperlinks** for terminals that support clickable links:
  ```
  \x1b]8;;{url}\x1b\\{display_text}\x1b]8;;\x1b\\
  ```
- Status bar: show link URL when the scroll position is on a line containing a link

**Standards note:** Not all terminals support OSC 8. Emit the escape sequence unconditionally —
non-supporting terminals simply ignore it. No feature detection needed.

---

## 6.2 — Footnotes

### Parser

New events:
- `Event::Start(Tag::FootnoteDefinition(label))` / `Event::End(TagEnd::FootnoteDefinition)` — accumulate definition content
- `Event::FootnoteReference(label)` — emit a superscript reference marker `[^n]`

### New IR variant

```rust
RenderedBlock::Footnote {
    label: String,
    content: Vec<RenderedBlock>,
}
```

### Layout

- Collect footnote definitions during parsing
- Append a footnote section at the end of the document:
  ```
  ──────────────
  [^1]: Footnote text here
  [^2]: Another footnote
  ```
- Add footnote positions to the heading index for navigation

---

## 6.3 — HTML Blocks

### Parser

- Handle `Event::Html(html)` and `Event::InlineHtml(html)`
- Strategy: strip HTML tags and render as dimmed plain text
- Alternative: skip entirely with a `[HTML block omitted]` placeholder

This is intentionally minimal — mdink is a markdown viewer, not an HTML renderer.

---

## 6.4 — Status Bar Enhancement

Current (Phase 1): `filename | scroll%`

Enhanced: `filename | current heading | scroll% | line/total`

### Current heading tracking

Add to `PreRenderedDocument`:
```rust
pub headings: Vec<(usize, u8, String)>  // (line_index, level, text)
```

Populate during `layout::flatten()` — whenever a `RenderedBlock::Heading` is flattened,
record its starting line index.

At render time, find the nearest heading above `scroll_offset`:
```rust
fn current_heading(headings: &[(usize, u8, String)], scroll_offset: usize) -> Option<&str> {
    headings.iter()
        .rev()
        .find(|(line, _, _)| *line <= scroll_offset)
        .map(|(_, _, text)| text.as_str())
}
```

---

## 6.5 — Search Mode

### UX

| Action | Key |
|--------|-----|
| Enter search mode | `/` |
| Type search query | alphanumeric keys |
| Next match | `n` |
| Previous match | `N` |
| Exit search mode | `Esc` |

All matches are highlighted simultaneously. The current match has a distinct highlight.

### Implementation

Add to `App`:
```rust
pub struct SearchState {
    pub active: bool,
    pub query: String,
    pub matches: Vec<usize>,       // line indices containing a match
    pub current_match: usize,      // index into `matches`
}
```

- On each keystroke in search mode: update `query`, re-scan `DocumentLine::Text` lines for substring matches
- `n`: advance `current_match`, scroll to that line
- `N`: decrement `current_match`, scroll to that line
- `Esc`: clear search state, remove highlights

### Rendering

- Matched text spans get a highlight style (from theme — add `search_highlight` to `MarkdownTheme`)
- The current match gets a brighter/inverted highlight
- The search input bar renders at the bottom, replacing the status bar during search mode

**Standards note:** Search state belongs in `App`. The renderer reads it but doesn't modify it.
(See [standards.md §2 — Single Responsibility](standards.md))

---

## 6.6 — Heading Navigation

| Key | Action |
|-----|--------|
| `Tab` | Jump to next heading |
| `Shift+Tab` | Jump to previous heading |

Uses the `headings` index from `PreRenderedDocument` (populated in §6.4).

```rust
// Next heading: first heading with line_index > scroll_offset
fn next_heading(headings: &[(usize, u8, String)], scroll_offset: usize) -> Option<usize> {
    headings.iter()
        .find(|(line, _, _)| *line > scroll_offset)
        .map(|(line, _, _)| *line)
}

// Previous heading: last heading with line_index < scroll_offset
fn prev_heading(headings: &[(usize, u8, String)], scroll_offset: usize) -> Option<usize> {
    headings.iter()
        .rev()
        .find(|(line, _, _)| *line < scroll_offset)
        .map(|(line, _, _)| *line)
}
```

---

## 6.7 — Keybindings Module (`src/keybindings.rs`)

Refactor all key handling from `app.rs` into a dedicated module.

### Action enum

```rust
pub enum Action {
    ScrollDown(usize),
    ScrollUp(usize),
    ScrollToTop,
    ScrollToBottom,
    NextHeading,
    PrevHeading,
    EnterSearch,
    ExitSearch,
    SearchInput(char),
    SearchBackspace,
    NextMatch,
    PrevMatch,
    Quit,
    None,
}
```

### Key mapping

```rust
pub fn map_key(key: KeyEvent, search_active: bool) -> Action
```

When `search_active`:
- Character keys → `SearchInput(c)`
- `Backspace` → `SearchBackspace`
- `Enter` / `Esc` → `ExitSearch`
- `n` → `NextMatch` (only after exiting search input)
- `N` → `PrevMatch`

When not `search_active`:
- All keybindings from Phase 1 + new ones:

| Key | Action |
|-----|--------|
| `j` / `↓` | `ScrollDown(1)` |
| `k` / `↑` | `ScrollUp(1)` |
| `d` / `PageDown` | `ScrollDown(half_page)` |
| `u` / `PageUp` | `ScrollUp(half_page)` |
| `g` / `Home` | `ScrollToTop` |
| `G` / `End` | `ScrollToBottom` |
| `Tab` | `NextHeading` |
| `Shift+Tab` | `PrevHeading` |
| `/` | `EnterSearch` |
| `n` | `NextMatch` |
| `N` | `PrevMatch` |
| `q` / `Esc` | `Quit` |

### App integration

```rust
// In app.rs
pub fn handle_action(&mut self, action: Action) {
    match action {
        Action::ScrollDown(n) => self.scroll_down(n),
        Action::Quit => self.quit = true,
        // ...
    }
}
```

---

## 6.8 — Input Sources

Extend `src/cli.rs` to support:

### File (already works)
```bash
mdink README.md
```

### Stdin
```bash
cat README.md | mdink -
echo "# Hello" | mdink
```

Detection: if `file == "-"` or stdin is not a TTY (`!atty::is(Stream::Stdin)`), read from stdin.

### URL (optional)
```bash
mdink https://raw.githubusercontent.com/user/repo/main/README.md
```

If URL support is desired, add `ureq = "3"` dependency. Detect URL by prefix (`https://` or `http://`), fetch content, render.

**Recommendation:** Defer URL support to post-v0.1 to keep the dependency tree small.

---

## 6.9 — Pager Mode (`--pager`)

Add to `src/cli.rs`:
```rust
/// Output styled markdown to stdout (no TUI)
#[arg(short = 'p', long)]
pub pager: bool,
```

When `--pager` is set:
1. Parse and flatten as normal
2. Instead of starting the TUI, iterate all `DocumentLine`s
3. Write ANSI-styled text to stdout using ratatui's style → ANSI conversion
4. Exit immediately

Useful for piping:
```bash
mdink -p README.md | less -R
mdink -p README.md > styled-output.txt
```

---

## 6.10 — Final Test Data

Create `testdata/full-featured.md` — a comprehensive document using **every** supported element:
- Headings h1–h6
- Paragraphs with bold, italic, strikethrough, inline code
- Fenced code blocks (multiple languages)
- Ordered and unordered lists (nested)
- Task lists
- Block quotes (nested)
- Tables with alignment
- Horizontal rules
- Links
- Images
- Footnotes
- HTML block

Ensure `cargo run -- testdata/full-featured.md` renders without panic.

---

## Phase 6 — Definition of Done

- [ ] Links render with underline + color from theme
- [ ] OSC 8 clickable hyperlinks emitted
- [ ] Footnotes collected and rendered at document bottom
- [ ] HTML blocks handled gracefully (stripped or placeholder)
- [ ] Status bar shows: filename | current heading | scroll% | line/total
- [ ] Search mode: `/` enters, type query, `n`/`N` navigate, `Esc` exits
- [ ] All matches highlighted simultaneously, current match distinct
- [ ] Heading navigation: `Tab` / `Shift+Tab` jump between headings
- [ ] Stdin input works: `echo "# Hello" | mdink -`
- [ ] `--pager` mode outputs styled text to stdout
- [ ] Keybindings centralized in `src/keybindings.rs` with `Action` enum
- [ ] `App` dispatches via `handle_action(action)`, not raw `KeyEvent`
- [ ] `testdata/full-featured.md` renders correctly
- [ ] No `unwrap()` on user input in any new code
- [ ] `cargo test` passes with all tests
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Phase 1–5 features still work (no regressions)
- [ ] Phase gate checklist from [standards.md §10](standards.md) passes

**Files created/modified:**
- Created: `src/keybindings.rs`, `testdata/full-featured.md`
- Modified: `src/parser.rs`, `src/layout.rs`, `src/renderer.rs`, `src/app.rs`, `src/main.rs`, `src/cli.rs`, `src/theme/mod.rs` (search_highlight style)
