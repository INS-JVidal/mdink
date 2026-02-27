# Phase 1: Minimal Viable Renderer

> **Prerequisites:** None (this is the first phase)
> **Standards:** All code must follow [standards.md](standards.md)
> **Depends on:** Nothing
> **Blocks:** All subsequent phases

**Goal:** Display a markdown file with styled text (headings, paragraphs, inline
formatting) and keyboard scrolling in a Ratatui TUI.

---

## 1.1 — Project Scaffold (packaging-aware from day one)

| Action | Detail |
|--------|--------|
| `cargo init --name mdink` | Create the crate |
| Write `Cargo.toml` | Full metadata + phase-1 deps (see below) |
| Create `LICENSE` | MIT (or dual MIT/Apache-2.0) — required for `.deb` copyright and crates.io |
| Create directory tree | `src/`, `testdata/`, `assets/`, `packaging/`, `.github/workflows/` |
| Create `testdata/basic.md` | Markdown file exercising paragraphs, headings (h1–h6), bold, italic, strikethrough, inline code, soft/hard breaks, horizontal rules |
| Create `.github/workflows/ci.yml` | Basic CI: `cargo build`, `cargo test`, `cargo clippy` |

### `Cargo.toml` — complete from Phase 1

```toml
[package]
name = "mdink"
version = "0.1.0"
edition = "2024"
rust-version = "1.86.0"
description = "A terminal markdown renderer with syntax highlighting and image support"
authors = ["Your Name <you@example.com>"]
license = "MIT"
homepage = "https://github.com/OWNER/mdink"
repository = "https://github.com/OWNER/mdink"
readme = "README.md"
keywords = ["markdown", "terminal", "tui", "renderer"]
categories = ["command-line-utilities"]

[dependencies]
# Phase 1
ratatui = { version = "0.30", features = ["crossterm"] }
pulldown-cmark = { version = "0.13", features = ["simd"] }
clap = { version = "4", features = ["derive"] }
unicode-width = "0.2"
textwrap = "0.16"
color-eyre = "0.6"
# Phase 2 (commented until needed)
# syntect = "5.2"
# Phase 4 (commented until needed)
# ratatui-image = { version = "10", default-features = false, features = ["image-defaults", "crossterm"] }
# image = "0.25"
# Phase 5 (commented until needed)
# serde = { version = "1", features = ["derive"] }
# serde_json = "1"
# dirs = "5"

# ── Release profile ──────────────────────────────────────────────
[profile.release]
opt-level = "s"
lto = "thin"
codegen-units = 1
strip = "debuginfo"        # keep symbol names for backtraces

# ── Distribution profile (used by cargo-deb and CI releases) ────
[profile.dist]
inherits = "release"
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"

# ── Debian packaging (cargo-deb reads this) ─────────────────────
[package.metadata.deb]
maintainer = "Your Name <you@example.com>"
copyright = "2026, Your Name <you@example.com>"
license-file = ["LICENSE", "0"]
extended-description = """\
mdink is a terminal-based markdown renderer inspired by glow. \
It features syntax-highlighted code blocks, inline terminal images \
(Sixel/Kitty/iTerm2), configurable JSON themes, and vim-style navigation."""
section = "utils"
priority = "optional"
depends = "$auto"
assets = [
    ["target/release/mdink",                  "usr/bin/",                                "755"],
    ["assets/mdink.1.gz",                     "usr/share/man/man1/",                     "644"],
    ["assets/completions/mdink.bash",         "usr/share/bash-completion/completions/",  "644"],
    ["assets/completions/mdink.fish",         "usr/share/fish/vendor_completions.d/",    "644"],
    ["assets/completions/_mdink",             "usr/share/zsh/vendor-completions/",       "644"],
    ["README.md",                             "usr/share/doc/mdink/README",              "644"],
]
```

### `.github/workflows/ci.yml`

```yaml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --locked
      - run: cargo test --locked
      - run: cargo clippy --locked -- -D warnings
```

**Files created:** `Cargo.toml`, `Cargo.lock`, `LICENSE`, `src/main.rs` (stub), `src/cli.rs`, `testdata/basic.md`, `.github/workflows/ci.yml`

---

## 1.2 — CLI Definition (`src/cli.rs`)

Separate from `main.rs` so Phase 7's xtask can import it for man page and completion generation.

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "mdink", version, about = "Terminal markdown renderer")]
pub struct Cli {
    /// Markdown file to render (use "-" for stdin)
    pub file: String,

    // Later phases will add: --style, --width, --pager, --no-images, --list-themes
}
```

**Standards note:** `cli.rs` must have **zero dependencies** beyond `clap`. No imports from
other mdink modules. This is a hard rule — it enables build-time reuse.
(See [standards.md §1.2](standards.md) — Module Boundaries)

---

## 1.3 — App Skeleton (`src/main.rs`, `src/app.rs`)

### `src/main.rs`

Thin orchestrator that wires the pipeline:

1. `color_eyre::install()?`
2. Set panic hook to restore terminal (see [standards.md §7.3](standards.md))
3. Parse CLI args via `Cli::parse()`
4. Read the markdown file to a `String`
5. Call `parser::parse(&source)` → `Vec<RenderedBlock>`
6. Call `layout::flatten(&blocks, terminal_width)` → `PreRenderedDocument`
7. Create `App` with the document
8. `ratatui::init()` (0.30 API)
9. Event loop: poll crossterm events → `app.handle_key()` → `renderer::draw()`
10. `ratatui::restore()`

### `src/app.rs`

Application state — pure data + logic, no rendering:

```rust
pub struct App {
    pub document: PreRenderedDocument,
    pub scroll_offset: usize,
    pub viewport_height: usize,
    pub filename: String,
    pub quit: bool,
}
```

Methods:
- `handle_key(key: KeyEvent)` — scroll / quit
- `visible_range() -> Range<usize>` — lines to render this frame
- `scroll_down(n: usize)`, `scroll_up(n: usize)`
- `scroll_to_top()`, `scroll_to_bottom()`
- `max_scroll() -> usize` — clamp upper bound (`total_height - viewport_height`)

**Standards note:** `App` manages **state only**. It never imports `ratatui::Frame` or
renders anything. The `renderer` reads from `&App`.
(See [standards.md §2 — Single Responsibility](standards.md))

### Keybindings (hardcoded in Phase 1, extracted in Phase 6)

| Key | Action |
|-----|--------|
| `j` / `↓` | scroll down 1 |
| `k` / `↑` | scroll up 1 |
| `d` / `PageDown` | scroll down half-page |
| `u` / `PageUp` | scroll up half-page |
| `g` / `Home` | top |
| `G` / `End` | bottom |
| `q` / `Esc` | quit |

---

## 1.4 — Markdown Parser (`src/parser.rs`)

### IR Types

```rust
/// A rendered markdown block ready for layout.
/// Each variant corresponds to a markdown block-level element.
pub enum RenderedBlock {
    Heading { level: u8, content: Vec<StyledSpan> },
    Paragraph { content: Vec<StyledSpan> },
    ThematicBreak,
    Spacer { lines: u16 },
}

/// A text span with style information.
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}
```

### Implementation: `pub fn parse(source: &str) -> Vec<RenderedBlock>`

- Create `pulldown_cmark::Parser` with `Options::ENABLE_STRIKETHROUGH | ENABLE_TABLES | ENABLE_TASKLISTS`
- Walk events with a state machine (see [standards.md §3.1](standards.md)):
  - Track a **style stack** (`Vec<Style>`) — push on `Start(Emphasis/Strong/Strikethrough)`, pop on `End`
  - Accumulate `StyledSpan`s into the current block's content vec
  - On `Start(Heading/Paragraph)` → begin new block
  - On `End(Heading/Paragraph)` → push completed block
  - `Event::Code(text)` → push `StyledSpan` with inline code style
  - `Event::Rule` → push `ThematicBreak`
  - `Event::SoftBreak` → push `StyledSpan { text: " " }`
  - `Event::HardBreak` → push `StyledSpan { text: "\n" }`
- Ignore unrecognized events (they'll be handled in later phases)

### Hardcoded Styles (Phase 1 only — replaced by theme in Phase 5)

Centralize these in a single `fn default_heading_style(level: u8) -> Style` function
so Phase 5 refactoring is a clean swap:

- h1: bold + bright cyan
- h2: bold + green
- h3: bold + yellow
- h4–h6: bold + white
- Bold: `Modifier::BOLD`
- Italic: `Modifier::ITALIC`
- Strikethrough: `Modifier::CROSSED_OUT`
- Inline code: dark gray bg + light gray fg
- Paragraph: default terminal fg/bg

**Standards note:** Use explicit match arms on `RenderedBlock`, never `_ =>`.
(See [standards.md §8.3](standards.md))

---

## 1.5 — Layout Engine (`src/layout.rs`)

### Types

```rust
pub struct PreRenderedDocument {
    pub lines: Vec<DocumentLine>,
    pub total_height: usize,
}

pub enum DocumentLine {
    Text(Line<'static>),
    Empty,
    Rule,
}
```

### Implementation: `pub fn flatten(blocks: &[RenderedBlock], width: u16) -> PreRenderedDocument`

For each block, compute how many terminal lines it needs:

- **Paragraph / Heading:** Convert `Vec<StyledSpan>` → ratatui `Line`, then use `textwrap` + `unicode-width` to wrap at given width. Emit the wrapped lines as `DocumentLine::Text`.
- **ThematicBreak:** Emit `DocumentLine::Rule` (1 line)
- **Spacer:** Emit N `DocumentLine::Empty` lines
- Add 1 `DocumentLine::Empty` between blocks (inter-block spacing)

**Standards note:** The wrapping challenge — `textwrap` operates on plain text but we need styled spans.
Strategy: wrap on plain text first, then re-apply styles to the wrapped segments by tracking character offsets.
(See risk register in [overview.md](overview.md))

---

## 1.6 — Renderer (`src/renderer.rs`)

### Implementation: `pub fn draw(frame: &mut Frame, app: &App)`

1. Calculate usable area (full terminal minus status bar — 1 row at bottom)
2. Determine visible lines from `app.visible_range()`
3. For each visible `DocumentLine`:
   - `Text(line)` → render with `Paragraph::new(line)` at the correct y offset
   - `Empty` → skip (empty row)
   - `Rule` → render a `"─".repeat(width)` styled line
4. Render status bar at bottom row: `filename | scroll% | line/total`

**Standards note:** `renderer.rs` never imports `pulldown_cmark`. It only sees `DocumentLine`.
(See [standards.md §1.2](standards.md))

---

## 1.7 — Wire Up and Test

- `cargo build` compiles cleanly
- `cargo run -- testdata/basic.md` — visually verify all elements
- `cargo clippy -- -D warnings` — clean
- Unit tests in `parser.rs`: known markdown → expected `RenderedBlock` variants
- Unit tests in `layout.rs`: known blocks → expected line count
- CI pipeline green

---

## Phase 1 — Definition of Done

- [ ] `cargo run -- testdata/basic.md` opens a TUI showing styled markdown
- [ ] Headings h1–h6 render with distinct colors
- [ ] Bold, italic, strikethrough, inline code render correctly
- [ ] Paragraphs word-wrap at terminal width
- [ ] Horizontal rules render as a line across the width
- [ ] All 7 keybindings work (j/k/d/u/g/G/q)
- [ ] Scroll is clamped (no scrolling past top/bottom)
- [ ] Status bar shows filename and scroll position
- [ ] Terminal is always restored on exit (including panic)
- [ ] `cargo test` passes with parser and layout unit tests
- [ ] `cargo clippy -- -D warnings` clean
- [ ] CI passes (`ci.yml` green)
- [ ] `Cargo.toml` has full metadata (license, description, homepage, repository)
- [ ] `[profile.dist]` and `[package.metadata.deb]` present (for Phase 7)
- [ ] `LICENSE` file exists
- [ ] `src/cli.rs` is separate from `main.rs` (zero non-clap deps)
- [ ] Hardcoded styles are centralized (not scattered)
- [ ] No `unwrap()` on user input
- [ ] All `match` on `RenderedBlock` / `DocumentLine` are exhaustive (no `_ =>`)

**Files created in this phase:**
`Cargo.toml`, `Cargo.lock`, `LICENSE`, `.github/workflows/ci.yml`,
`src/main.rs`, `src/cli.rs`, `src/app.rs`, `src/parser.rs`, `src/layout.rs`, `src/renderer.rs`,
`testdata/basic.md`
