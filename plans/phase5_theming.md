# Phase 5: JSON Theming System

> **Prerequisites:** Phase 4 complete
> **Standards:** All code must follow [standards.md](standards.md)
> **New dependencies:** `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `dirs = "5"`

**Goal:** Externalize all styling to JSON theme files. Provide 3 built-in themes. Replace
every hardcoded color with theme-driven styling.

**This is the most invasive refactor.** It touches every module that produces styles.
(See cross-phase notes in [overview.md](overview.md))

---

## 5.1 — Theme Data Types (`src/theme/mod.rs`)

All structs derive `Deserialize` and `Clone`:

```rust
#[derive(Deserialize, Clone)]
pub struct MarkdownTheme {
    pub name: String,
    pub document: DocumentStyle,
    pub heading: [HeadingStyle; 6],
    pub paragraph: BlockStyle,
    pub code_block: CodeBlockStyle,
    pub block_quote: BlockQuoteStyle,
    pub table: TableStyle,
    pub thematic_break: ThematicBreakStyle,
    pub list: ListStyle,
    pub emphasis: InlineStyle,
    pub strong: InlineStyle,
    pub strikethrough: InlineStyle,
    pub code_inline: InlineStyle,
    pub link: LinkStyle,
    pub image_alt: InlineStyle,
    pub syntect_theme: String,
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
    pub prefix: Option<String>,
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
    pub language_tag: bool,
}

#[derive(Deserialize, Clone)]
pub struct BlockQuoteStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub border_fg: Option<String>,
    pub prefix: String,
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
> arrays from JSON **arrays**, not objects. Theme files must use a positional array:
>
> ```json
> "heading": [
>   { "fg": "#00ffff", "bold": true, "italic": false, "underline": false, "prefix": "# ", "margin_top": 2, "margin_bottom": 1 },
>   { "fg": "#00ff00", "bold": true, "italic": false, "underline": false, "prefix": "## ", "margin_top": 1, "margin_bottom": 1 },
>   ...
> ]
> ```
>
> This differs from glamour's named-key approach (`"h1": {...}`). The positional format
> maps directly to the Rust `[HeadingStyle; 6]` type. Document this in built-in theme
> JSON files and in the example custom theme.

### Color parsing

```rust
pub fn parse_color(s: &str) -> Option<Color>
```

| Input | Output |
|-------|--------|
| `"#ff5500"` | `Color::Rgb(255, 85, 0)` |
| `"99"` | `Color::Indexed(99)` |
| `"red"` | `Color::Red` |
| `""` or missing | `None` (terminal default) |

### Style conversion

Use a trait or helper to convert theme elements to `ratatui::Style`:

```rust
pub fn heading_style(theme: &MarkdownTheme, level: u8) -> Style {
    let h = &theme.heading[level as usize - 1];
    let mut style = Style::default();
    if let Some(ref fg) = h.fg { style = style.fg(parse_color(fg).unwrap_or_default()); }
    if let Some(ref bg) = h.bg { style = style.bg(parse_color(bg).unwrap_or_default()); }
    if h.bold { style = style.add_modifier(Modifier::BOLD); }
    if h.italic { style = style.add_modifier(Modifier::ITALIC); }
    if h.underline { style = style.add_modifier(Modifier::UNDERLINED); }
    style
}
```

**Standards note:** Theme functions accept the **narrowest type** they need.
`heading_style` takes `&MarkdownTheme` (or better: `&HeadingStyle`), not `&App`.
(See [standards.md §2 — SOLID/I](standards.md) — Interface Segregation)

---

## 5.2 — Built-in Themes

Three JSON files embedded via `include_str!()`:

| File | Description |
|------|-------------|
| `src/theme/dark.json` | Dark background, bright heading colors, muted paragraph text |
| `src/theme/light.json` | Light background, darker heading colors |
| `src/theme/dracula.json` | Dracula color palette (purple, green, pink, cyan) |

```rust
const DARK_THEME: &str = include_str!("dark.json");
const LIGHT_THEME: &str = include_str!("light.json");
const DRACULA_THEME: &str = include_str!("dracula.json");
```

Each JSON file must validate against the `MarkdownTheme` struct at compile time
(tested by unit tests, not macro-enforced).

---

## 5.3 — Theme Loading

```rust
pub fn load_theme(name_or_path: &str) -> Result<MarkdownTheme, ThemeError>
```

Resolution order:
1. Built-in name match (`"dark"`, `"light"`, `"dracula"`) → deserialize embedded JSON
2. File path exists → read file, deserialize JSON
3. Check `~/.config/mdink/themes/{name}.json` → read file, deserialize JSON
4. `Err(ThemeError::NotFound { name })`

Default theme selection:
1. Check `MDINK_STYLE` env var
2. Fall back to `"dark"`

**Standards note:** Use a domain-specific `ThemeError` enum, not `String` errors.
(See [standards.md §4.2](standards.md))

```rust
#[derive(Debug)]
pub enum ThemeError {
    NotFound { name: String },
    InvalidColor { value: String },
    ParseError { source: serde_json::Error },
    IoError { source: std::io::Error },
}
```

---

## 5.4 — Refactor: Thread Theme Through Pipeline

This is the big change. Every function that was using hardcoded styles now receives
`&MarkdownTheme` (or a subsection of it).

### Modules affected

| Module | Change |
|--------|--------|
| `parser.rs` | `parse(source, highlighter, image_manager, &theme)` — inline styles (bold, italic, code_inline) now come from theme |
| `layout.rs` | `flatten(blocks, width, &theme)` — spacing/margins from `theme.document`, heading margins from `theme.heading[n]` |
| `renderer.rs` | `draw(frame, app, image_manager)` — status bar styling, document bg from `theme.document` |
| `highlight.rs` | `highlight_code(code, lang, &theme.syntect_theme)` — syntect theme name from JSON |
| `app.rs` | `App` now holds `theme: MarkdownTheme` |

### Migration strategy

1. Add `theme: MarkdownTheme` to `App`
2. Find every place that constructs a `Style` with hardcoded colors
3. Replace with the corresponding theme function call
4. Remove the centralized `default_heading_style()` from Phase 1 (it's now in `theme/mod.rs`)
5. Verify: `grep -rn "Color::" src/` should find zero hits outside `theme/mod.rs`

**Standards note:** After this refactor, there must be **zero hardcoded colors** in the
renderer, parser, or layout. All styling flows from the theme.
(See [standards.md §8.2](standards.md))

---

## 5.5 — CLI Integration

Add to `src/cli.rs`:

```rust
/// Theme: dark, light, dracula, or path to JSON file
#[arg(short = 's', long = "style", default_value = "dark")]
pub style: String,

/// List available built-in themes and exit
#[arg(long)]
pub list_themes: bool,
```

Environment variable: `MDINK_STYLE` — checked in `main.rs` if `--style` is not explicitly provided.

`--list-themes`: print theme names to stdout and exit:
```
Built-in themes:
  dark      Dark background with bright colors (default)
  light     Light background with muted colors
  dracula   Dracula color palette

Custom themes:
  Place .json files in ~/.config/mdink/themes/
```

---

## 5.6 — Tests

### Unit tests in `theme/mod.rs`

- Parse each built-in JSON → valid `MarkdownTheme` (no deserialization errors)
- `parse_color("#ff5500")` → `Color::Rgb(255, 85, 0)`
- `parse_color("99")` → `Color::Indexed(99)`
- `parse_color("red")` → `Color::Red`
- `parse_color("")` → `None`
- `parse_color("invalid")` → `None` or error
- `load_theme("dark")` → Ok
- `load_theme("nonexistent")` → Err(NotFound)
- `heading_style` produces non-default `Style` for each heading level

### Integration tests

- Render `testdata/basic.md` with each built-in theme without panic
- Render with a custom theme JSON file
- Render with `MDINK_STYLE=dracula` env var

---

## Phase 5 — Definition of Done

- [ ] 3 built-in themes load correctly (dark, light, dracula)
- [ ] Custom themes load from file path
- [ ] Custom themes load from `~/.config/mdink/themes/`
- [ ] `-s` / `--style` flag selects theme
- [ ] `MDINK_STYLE` env var selects theme
- [ ] `--list-themes` prints available themes and exits
- [ ] **All hardcoded styles replaced** — `grep -rn "Color::" src/` finds zero hits outside `theme/`
- [ ] Color parsing handles hex, ANSI-256, named colors, and empty/missing
- [ ] `ThemeError` enum used for all theme-related errors (no string errors)
- [ ] Theme functions accept narrowest type needed (not `&App`)
- [ ] syntect theme name configurable via `theme.syntect_theme`
- [ ] `cargo test` passes with theme tests
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Phase 1–4 features still work (no regressions)
- [ ] Phase gate checklist from [standards.md §10](standards.md) passes

**Files created/modified:**
- Created: `src/theme/mod.rs`, `src/theme/dark.json`, `src/theme/light.json`, `src/theme/dracula.json`
- Modified: `Cargo.toml` (uncomment serde, serde_json, dirs), `src/parser.rs`, `src/layout.rs`, `src/renderer.rs`, `src/highlight.rs`, `src/app.rs`, `src/main.rs`, `src/cli.rs`
