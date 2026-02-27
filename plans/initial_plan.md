# mdink: Terminal Markdown Renderer with Image Support

## Project Vision

Build a terminal-based markdown renderer in Rust inspired by [charmbracelet/glow](https://github.com/charmbracelet/glow), using Ratatui as the TUI framework. The tool renders markdown files with full styling, syntax-highlighted code blocks, inline terminal images, and a configurable JSON theming system.

**Pipeline:** `Markdown source → pulldown-cmark parser → Custom renderer → Ratatui TUI`

---

## Architecture

### Crate Dependencies

> **Interoperability note (Feb 2026 audit):** Ratatui 0.30.0 introduced a modular workspace reorganization. The `syntect-tui` crate (v3.0.6) still depends on `ratatui ^0.29` and is **incompatible** with ratatui 0.30. Since the syntect→ratatui bridge is trivial (~20 lines), we implement it directly in `highlight.rs` instead of depending on `syntect-tui`. This ensures all crates resolve to the same `ratatui 0.30` dependency tree.

```toml
[package]
name = "mdink"
version = "0.1.0"
edition = "2024"
rust-version = "1.86.0"

[dependencies]
# TUI framework (0.30 = modular workspace release, Dec 2025)
# crossterm is re-exported via ratatui — no separate crossterm dependency needed
ratatui = { version = "0.30", features = ["crossterm"] }

# Markdown parsing (CommonMark + GFM extensions)
pulldown-cmark = { version = "0.13", features = ["simd"] }

# Syntax highlighting for code blocks
# NOTE: Do NOT use syntect-tui (stuck on ratatui ^0.29, incompatible with 0.30)
# We implement the ~20-line syntect→ratatui::Span bridge in highlight.rs
syntect = "5.2"

# Terminal image rendering (Sixel, Kitty, iTerm2, halfblocks, chafa fallback)
# v10 requires ratatui ^0.30 — version-aligned with our ratatui dep
ratatui-image = { version = "10", default-features = false, features = ["image-defaults", "crossterm"] }
image = "0.25"

# Theming
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# CLI
clap = { version = "4", features = ["derive"] }

# Utilities
unicode-width = "0.2"
textwrap = "0.16"
dirs = "5"

# Error handling
color-eyre = "0.6"
```

**Dependency compatibility matrix:**

| Crate | Version | Depends on ratatui | Notes |
|-------|---------|-------------------|-------|
| ratatui | 0.30.0 | — | Modular workspace; crossterm 0.29 via feature |
| ratatui-image | 10.0.x | ^0.30.0 | Aligned ✅ |
| ~~syntect-tui~~ | ~~3.0.6~~ | ~~^0.29.0~~ | **REMOVED** — incompatible, trivial to replace |
| pulldown-cmark | 0.13.0 | — | No ratatui dependency |
| syntect | 5.2.x | — | No ratatui dependency |

### Ratatui 0.30 Migration Notes

Ratatui 0.30 introduced breaking changes relevant to this project:

1. **Simplified init/restore:** Use `ratatui::init()` / `ratatui::restore()` (the only public init/restore functions in 0.30)
2. **Modular workspace:** Core types in `ratatui-core`, widgets in `ratatui-widgets`, but the main `ratatui` crate re-exports everything
3. **TestBackend moved:** Testing uses `ratatui::backend::TestBackend` (still available in main crate)
4. **crossterm re-exported:** Access crossterm via `ratatui::crossterm` instead of direct dependency

### Project Structure

```
mdink/
├── Cargo.toml
├── src/
│   ├── main.rs                 # CLI entry point, arg parsing, event loop
│   ├── app.rs                  # App state: scroll position, viewport, mode
│   ├── parser.rs               # pulldown-cmark event stream → RenderedBlock
│   ├── renderer.rs             # RenderedBlock → Frame (ratatui rendering)
│   ├── highlight.rs            # syntect integration + syntect→ratatui bridge
│   ├── images.rs               # ratatui-image: load, cache, render images
│   ├── theme/
│   │   ├── mod.rs              # Theme struct, loading, Style conversion
│   │   ├── dark.json           # Built-in dark theme
│   │   ├── light.json          # Built-in light theme
│   │   └── dracula.json        # Built-in dracula theme
│   ├── layout.rs               # Block measurement & vertical space allocation
│   └── keybindings.rs          # Input handling (vim-style + standard)
├── themes/                     # User-facing theme examples
│   └── example-custom.json
├── testdata/                   # Sample markdown files for testing
│   ├── basic.md
│   ├── code-blocks.md
│   ├── tables.md
│   ├── images.md
│   └── full-featured.md
└── README.md
```

---

## Core Data Types

### RenderedBlock (parser output)

The parser converts the pulldown-cmark event stream into a flat `Vec<RenderedBlock>`:

```rust
/// A rendered markdown block ready for layout and display.
/// This is the intermediate representation between parsing and rendering.
pub enum RenderedBlock {
    /// Heading with level (1-6) and styled content
    Heading {
        level: u8,
        content: Vec<StyledSpan>,
    },

    /// Normal paragraph with inline styling
    Paragraph {
        content: Vec<StyledSpan>,
    },

    /// Fenced or indented code block with syntax highlighting
    CodeBlock {
        language: String,
        highlighted_lines: Vec<Line<'static>>,  // Pre-highlighted via syntect
    },

    /// Block quote (may contain nested blocks)
    BlockQuote {
        children: Vec<RenderedBlock>,
    },

    /// Ordered or unordered list
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<ListItem>,
    },

    /// Table with headers and rows
    Table {
        headers: Vec<Vec<StyledSpan>>,
        alignments: Vec<Alignment>,
        rows: Vec<Vec<Vec<StyledSpan>>>,
    },

    /// Horizontal rule / thematic break
    ThematicBreak,

    /// Inline image — stores raw image data for deferred protocol creation.
    /// The actual `StatefulProtocol` is materialized during layout and stored
    /// in `App::image_protocols: Vec<StatefulProtocol>` (indexed by `protocol_index`).
    /// This avoids holding a `!Clone` type in the IR and sidesteps borrow-checker
    /// conflicts during rendering.
    Image {
        protocol_index: usize,  // Index into App::image_protocols
        alt_text: String,
        title: String,
        width_cells: u16,
        height_cells: u16,
    },

    /// Fallback when image cannot be loaded or terminal has no graphics support
    ImageFallback {
        alt_text: String,
    },

    /// Task list item (GitHub Flavored Markdown)
    TaskListItem {
        checked: bool,
        content: Vec<StyledSpan>,
    },

    /// Footnote definition
    Footnote {
        label: String,
        content: Vec<RenderedBlock>,
    },

    /// Empty vertical spacing
    Spacer { lines: u16 },
}

pub struct ListItem {
    pub content: Vec<StyledSpan>,
    pub children: Vec<RenderedBlock>,  // Nested lists
}

/// A text span with style information
pub struct StyledSpan {
    pub text: String,
    pub style: Style,      // ratatui::style::Style
}
```

### Theme (JSON-configurable)

```rust
/// JSON-serializable theme matching glamour's approach.
/// Colors accept: ANSI 256 numbers ("99"), hex ("#ff5500"), or names ("red").
#[derive(Deserialize, Clone)]
pub struct MarkdownTheme {
    pub name: String,

    // Document
    pub document: DocumentStyle,

    // Block elements
    pub heading: [HeadingStyle; 6],  // h1 through h6
    pub paragraph: BlockStyle,
    pub code_block: CodeBlockStyle,
    pub block_quote: BlockQuoteStyle,
    pub table: TableStyle,
    pub thematic_break: ThematicBreakStyle,
    pub list: ListStyle,

    // Inline elements
    pub emphasis: InlineStyle,       // *italic*
    pub strong: InlineStyle,         // **bold**
    pub strikethrough: InlineStyle,  // ~~strike~~
    pub code_inline: InlineStyle,    // `code`
    pub link: LinkStyle,
    pub image_alt: InlineStyle,      // Alt text when image can't render

    // Code highlighting
    pub syntect_theme: String,       // syntect theme name: "base16-ocean.dark", etc.
}

#[derive(Deserialize, Clone)]
pub struct DocumentStyle {
    pub margin_left: u16,
    pub margin_right: u16,
    pub bg: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct HeadingStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub prefix: Option<String>,      // e.g. "# ", "## "
    pub margin_top: u16,
    pub margin_bottom: u16,
}

#[derive(Deserialize, Clone)]
pub struct CodeBlockStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub border: bool,
    pub margin: u16,
    pub line_numbers: bool,
    pub language_tag: bool,          // Show language label
}

#[derive(Deserialize, Clone)]
pub struct BlockQuoteStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub border_fg: Option<String>,
    pub prefix: String,              // e.g. "│ "
    pub italic: bool,
}

#[derive(Deserialize, Clone)]
pub struct BlockStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub margin_top: u16,
    pub margin_bottom: u16,
}

#[derive(Deserialize, Clone)]
pub struct InlineStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Deserialize, Clone)]
pub struct LinkStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub underline: bool,
}

#[derive(Deserialize, Clone)]
pub struct TableStyle {
    pub header_fg: Option<String>,
    pub header_bg: Option<String>,
    pub header_bold: bool,
    pub row_fg: Option<String>,
    pub row_alt_bg: Option<String>,  // Alternating row background
    pub border_fg: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct ThematicBreakStyle {
    pub fg: Option<String>,
    pub char: String,                // e.g. "─", "━", "—"
}

#[derive(Deserialize, Clone)]
pub struct ListStyle {
    pub bullet_fg: Option<String>,
    pub number_fg: Option<String>,
    pub task_checked_fg: Option<String>,
    pub task_unchecked_fg: Option<String>,
    pub indent_size: u16,            // Spaces per nesting level (default: 2)
}
```

> **JSON representation of `heading: [HeadingStyle; 6]`:** Serde deserializes fixed-size
> arrays from JSON **arrays** (not objects). Theme files must use a positional array:
>
> ```json
> "heading": [
>   { "fg": "#00ffff", "bold": true, ... },
>   { "fg": "#00ff00", "bold": true, ... },
>   ...
> ]
> ```
>
> This differs from glamour's named-key approach (`"h1": {...}`). The positional format
> is intentional — it avoids redundant key parsing and maps directly to the `[HeadingStyle; 6]`
> type. Document this in theme examples.

---

## Implementation Stages

### Stage 1: Minimal Viable Renderer

**Goal:** Render a markdown file with basic text styling and scrolling.

**Tasks:**
1. Set up the Ratatui app skeleton using `ratatui::init()` / `ratatui::restore()` (0.30 API)
2. Implement `parser.rs`: walk pulldown-cmark events and produce `Vec<RenderedBlock>` for:
   - Paragraphs with inline styles (bold, italic, strikethrough, inline code)
   - Headings (h1-h6) with distinct colors
   - Thematic breaks (horizontal rules)
   - Soft/hard line breaks
3. Implement `renderer.rs`: convert `RenderedBlock` → ratatui `Paragraph` widgets with word wrapping
4. Implement basic scrolling (up/down/page-up/page-down/home/end)
5. Hardcoded dark theme

**Key pulldown-cmark events to handle:**

```rust
Event::Start(Tag::Heading { level, .. })
Event::End(TagEnd::Heading(_))
Event::Start(Tag::Paragraph)
Event::End(TagEnd::Paragraph)
Event::Start(Tag::Emphasis)          // *italic*
Event::End(TagEnd::Emphasis)
Event::Start(Tag::Strong)            // **bold**
Event::End(TagEnd::Strong)
Event::Start(Tag::Strikethrough)     // ~~strike~~
Event::End(TagEnd::Strikethrough)
Event::Code(text)                    // `inline code`
Event::Text(text)                    // Plain text content
Event::SoftBreak
Event::HardBreak
Event::Rule                          // ---
```

**Keybindings for Stage 1:**
- `j` / `↓` : scroll down
- `k` / `↑` : scroll up
- `d` / `Page Down` : half page down
- `u` / `Page Up` : half page up
- `g` / `Home` : top
- `G` / `End` : bottom
- `q` / `Esc` : quit

### Stage 2: Code Blocks with Syntax Highlighting

**Goal:** Render fenced code blocks with syntect highlighting.

**Tasks:**
1. Implement `highlight.rs` with syntect integration AND the syntect→ratatui bridge
2. Detect language from code fence info string (` ```rust `)
3. Fall back to plain text if language not recognized
4. Render code blocks with:
   - Background color (from theme)
   - Optional left border
   - Optional line numbers
   - Language label in top-right corner
5. Load syntect SyntaxSet and ThemeSet once at startup (they're expensive)

**The syntect→ratatui bridge (replaces syntect-tui):**

```rust
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::highlighting::Style as SyntectStyle;

/// Convert a syntect highlighted segment into a ratatui Span.
/// This replaces the syntect-tui crate with ~15 lines.
pub fn syntect_style_to_span(
    text: &str,
    style: SyntectStyle,
) -> Span<'static> {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    let bg = Color::Rgb(style.background.r, style.background.g, style.background.b);
    let mut modifier = Modifier::empty();
    if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
        modifier |= Modifier::BOLD;
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
        modifier |= Modifier::ITALIC;
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
        modifier |= Modifier::UNDERLINED;
    }
    Span::styled(text.to_string(), Style::default().fg(fg).bg(bg).add_modifier(modifier))
}
```

**Key pulldown-cmark events:**

```rust
Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
Event::Start(Tag::CodeBlock(CodeBlockKind::Indented))
Event::Text(code_content)   // Inside code block
Event::End(TagEnd::CodeBlock)
```

### Stage 3: Lists, Block Quotes, and Tables

**Goal:** Handle structured block elements.

**Tasks:**
1. **Lists:** Handle ordered/unordered, nesting, task lists
   - Track list depth for indentation
   - Ordered lists: number prefix with correct start value
   - Unordered lists: bullet character (•, ◦, ▪ by depth)
   - Task lists: ☑ / ☐ prefix
2. **Block quotes:** Render with vertical bar prefix (│), dimmed/italic text
   - Handle nested block quotes (increase prefix)
3. **Tables:** Render using ratatui's Table widget
   - Parse column alignments from pulldown-cmark
   - Calculate column widths based on content
   - Style header row distinctly

**Key events:**

```rust
// Lists
Event::Start(Tag::List(first_number))     // None = unordered, Some(n) = ordered
Event::Start(Tag::Item)
Event::TaskListMarker(checked)

// Block quotes
Event::Start(Tag::BlockQuote(kind))

// Tables
Event::Start(Tag::Table(alignments))
Event::Start(Tag::TableHead)
Event::Start(Tag::TableRow)
Event::Start(Tag::TableCell)
```

### Stage 4: Terminal Image Support

**Goal:** Render inline images using terminal graphics protocols.

**Tasks:**
1. Implement `images.rs` with ratatui-image v10 integration
2. Auto-detect terminal graphics protocol at startup via `Picker::from_query_stdio()`
3. Load images from:
   - Local file paths (relative to markdown file location)
   - Optionally: HTTP URLs (download to temp cache)
4. Scale images to fit terminal width (respecting theme margins)
5. Fall back to `[alt text]` display when:
   - Terminal doesn't support any graphics protocol
   - Image file not found
   - Image decode error
6. Use `StatefulImage` for adaptive rendering with resize support

**ratatui-image v10 API changes (vs. v4):**

```rust
// v10 API — StatefulProtocol is now a concrete type, not Box<dyn>
use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol};

// Query terminal capabilities
let mut picker = Picker::from_query_stdio()?;

// Load and create protocol
let dyn_img = image::ImageReader::open(path)?.decode()?;
let protocol: StatefulProtocol = picker.new_resize_protocol(dyn_img);

// Render with StatefulImage widget
let image_widget = StatefulImage::default();
frame.render_stateful_widget(image_widget, area, &mut protocol);

// Check encoding result (recommended)
protocol.last_encoding_result().unwrap()?;
```

**Key pulldown-cmark events:**

```rust
Event::Start(Tag::Image { link_type, dest_url, title, id })
Event::Text(alt_text)    // Inside image tag
Event::End(TagEnd::Image)
```

**Supported terminal protocols (via ratatui-image v10):**
- Sixels (xterm, foot, mlterm, WezTerm)
- Kitty graphics protocol (kitty, WezTerm, Ghostty)
- iTerm2 inline images (iTerm2, WezTerm)
- Chafa fallback (new in v10 — renders via libchafa without protocol support)
- Halfblocks fallback (any terminal with Unicode support)

### Stage 5: JSON Theming System

**Goal:** Load themes from JSON files, matching glamour's approach.

**Tasks:**
1. Define the `MarkdownTheme` struct with serde
2. Create 3 built-in themes as embedded JSON (dark, light, dracula)
3. Auto-detect terminal background (dark/light) for default theme selection
4. Load custom themes from:
   - CLI flag: `mdink -s mytheme.json README.md`
   - Environment variable: `MDINK_STYLE=dracula`
   - Config file: `~/.config/mdink/config.toml`
5. Convert theme color strings to ratatui `Color`:
   - ANSI 256: `"99"` → `Color::Indexed(99)`
   - Hex: `"#ff5500"` → `Color::Rgb(255, 85, 0)`
   - Named: `"red"` → `Color::Red`
6. Apply theme at render time — all rendering functions receive `&MarkdownTheme`

### Stage 6: Links, Footnotes, and Polish

**Goal:** Handle remaining elements and polish UX.

**Tasks:**
1. **Links:** Render with underline + color, show URL in status bar on hover/select
2. **OSC 8 hyperlinks:** For terminals that support clickable links
3. **Footnotes:** Collect and render at document bottom
4. **HTML blocks:** Strip or render as dimmed raw text
5. **Status bar:** Show filename, scroll position (%), current heading
6. **Search:** `/` to enter search mode, `n`/`N` to navigate matches
7. **Heading navigation:** `Tab`/`Shift+Tab` to jump between headings
8. **Input sources:**
   - File: `mdink README.md`
   - Stdin: `cat README.md | mdink -`
   - URL: `mdink https://example.com/doc.md`

---

## Layout Engine Design

The layout engine is the hardest part. It must calculate how many terminal rows each block occupies **before** rendering, to enable accurate scrolling.

### Pre-rendering approach (recommended, glow uses this)

```rust
/// Pre-rendered document: all blocks measured and flattened into lines.
pub struct PreRenderedDocument {
    /// Every line of the document, ready to render
    lines: Vec<DocumentLine>,
    /// Index: heading positions for navigation
    headings: Vec<(usize, u8, String)>,  // (line_index, level, text)
    /// Total height in terminal lines
    total_height: usize,
}

pub enum DocumentLine {
    /// A styled text line (paragraph, heading, list item, etc.)
    Text(Line<'static>),
    /// A line within a code block (pre-highlighted)
    Code(Line<'static>),
    /// An image occupying N rows (holds mutable StatefulProtocol)
    ImageStart {
        protocol_index: usize,  // Index into a Vec<StatefulProtocol> held by App
        height: u16,
    },
    ImageContinuation,  // Placeholder lines for image height
    /// An empty line (spacing between blocks)
    Empty,
    /// Horizontal rule
    Rule,
}
```

> **Design note on images — two-pass pipeline:**
>
> 1. **Parse pass:** The parser encounters `Tag::Image`, attempts to load the image via
>    `ImageManager`, and stores the resulting `protocol_index` (a `usize`) in
>    `RenderedBlock::Image`. If loading fails, it emits `RenderedBlock::ImageFallback` instead.
>    The actual `StatefulProtocol` object is stored in `ImageManager::protocols: Vec<StatefulProtocol>`.
>
> 2. **Render pass:** `DocumentLine::ImageStart` stores the same `protocol_index`. At draw
>    time, the renderer retrieves `&mut StatefulProtocol` from `App`'s `ImageManager` by
>    index, avoiding borrow-checker conflicts when iterating lines and rendering simultaneously.
>
> `StatefulProtocol` is `!Clone` and requires `&mut` access. It is **never** stored in
> the IR or the pre-rendered document — only an index is stored. This is the key invariant.

### Rendering flow

```
1. Parse markdown → Vec<RenderedBlock>
2. Measure each block → calculate line count per block
3. Flatten blocks → Vec<DocumentLine> (the pre-rendered document)
4. On each frame:
   a. Determine visible range: lines[scroll_offset..scroll_offset + viewport_height]
   b. Render only visible lines to Frame
   c. Render scrollbar
   d. Render status bar
```

---

## CLI Interface

```
mdink - Terminal Markdown Renderer

USAGE:
    mdink [OPTIONS] [FILE]

ARGS:
    <FILE>    Markdown file to render (use "-" for stdin)

OPTIONS:
    -s, --style <THEME>     Theme: dark, light, dracula, or path to JSON
    -w, --width <COLS>      Max rendering width (default: terminal width)
    -p, --pager             Use pager mode (no TUI, just styled output)
    --no-images             Disable image rendering
    --list-themes           Show available built-in themes
    -h, --help              Print help
    -V, --version           Print version
```

---

## Testing Strategy

1. **Unit tests per module:**
   - `parser.rs`: Known markdown → expected `Vec<RenderedBlock>`
   - `highlight.rs`: Code string + language → non-empty highlighted lines
   - `theme/mod.rs`: JSON → valid `MarkdownTheme`, color string → `Color`
   - `layout.rs`: Blocks → expected line counts

2. **Integration tests:**
   - Render `testdata/*.md` files without panicking
   - Verify scroll bounds
   - Theme loading from file

3. **Visual testing:**
   - Use ratatui's `TestBackend` to capture rendered output
   - Snapshot test rendered buffers against expected output

---

## Reference Projects

| Project | Language | What to learn from it |
|---------|----------|----------------------|
| [glow](https://github.com/charmbracelet/glow) | Go | Overall UX, TUI layout, file browser |
| [glamour](https://github.com/charmbracelet/glamour) | Go | Theming JSON format, element styling approach |
| [tui-markdown](https://github.com/joshka/tui-markdown) | Rust | pulldown-cmark → ratatui Text conversion patterns |
| [mdfried](https://github.com/benjajaja/mdfried) | Rust | Markdown viewer with images (same author as ratatui-image) |
| [ratatui-image](https://github.com/benjajaja/ratatui-image) | Rust | Terminal image protocol integration |

---

## Feasibility Assessment

| Feature | Difficulty | Status |
|---------|------------|--------|
| Text styling (bold, italic, headings) | Easy | `tui-markdown` already does this |
| Code block syntax highlighting | Easy | `syntect` + custom 15-line bridge |
| Tables | Medium | Ratatui's `Table` widget handles it |
| Images in terminal | Medium | `ratatui-image` v10 handles Sixel/Kitty/iTerm2/chafa |
| Scrolling | Easy | Ratatui `Scrollbar` + offset tracking |
| JSON theming | Easy | `serde_json` + style conversion |
| Links (clickable) | Medium | OSC 8 hyperlinks, terminal support varies |
| Math rendering | Hard | Would need custom rendering |

---

## Non-Goals (for v0.1)

- Markdown editing (read-only viewer)
- File browser / stash system (glow's TUI mode)
- Remote sync / cloud storage
- PDF export
- Custom markdown extensions beyond GFM
