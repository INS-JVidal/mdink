# Phase 3: Lists, Block Quotes, and Tables

> **Prerequisites:** Phase 2 complete
> **Standards:** All code must follow [standards.md](standards.md)
> **New dependencies:** None

**Goal:** Handle structured block-level elements with proper nesting and indentation.

---

## 3.1 — Parser: Lists

### New IR variants

```rust
RenderedBlock::List {
    ordered: bool,
    start: Option<u64>,
    items: Vec<ListItem>,
}

RenderedBlock::TaskListItem {
    checked: bool,
    content: Vec<StyledSpan>,
}

pub struct ListItem {
    pub content: Vec<StyledSpan>,
    pub children: Vec<RenderedBlock>,  // Nested lists
}
```

### Parser state machine extension

New states:
```rust
ParserState::InList { depth: u8, ordered: bool, counter: u64 }
ParserState::InListItem
```

Use the **state stack** (`Vec<ParserState>`) to track nesting depth. Lists can contain
lists, block quotes, paragraphs, and code blocks — all of which need recursive handling.

Events:
- `Event::Start(Tag::List(first_number))` → push `InList` state. `None` = unordered, `Some(n)` = ordered starting at n
- `Event::Start(Tag::Item)` → push `InListItem` state, begin accumulating content
- `Event::TaskListMarker(checked)` → mark current item as task list item
- `Event::End(TagEnd::Item)` → pop state, push completed item to current list
- `Event::End(TagEnd::List(_))` → pop state, push completed `List` block

### Bullet characters by depth

| Depth | Unordered | Ordered |
|-------|-----------|---------|
| 0 | `•` | `1.` |
| 1 | `◦` | `1.` |
| 2+ | `▪` | `1.` |

Task list prefixes: `☑` (checked) / `☐` (unchecked)

---

## 3.2 — Parser: Block Quotes

### New IR variant

```rust
RenderedBlock::BlockQuote {
    children: Vec<RenderedBlock>,
}
```

### Parser state machine extension

New state:
```rust
ParserState::InBlockQuote { depth: u8 }
```

- `Event::Start(Tag::BlockQuote(_))` → push state, increment depth
- Recursively parse inner events into `children: Vec<RenderedBlock>`
- `Event::End(TagEnd::BlockQuote)` → pop state, push completed `BlockQuote`

Block quotes can nest arbitrarily. Each nesting level adds another `│ ` prefix.

---

## 3.3 — Parser: Tables

### New IR variant

```rust
RenderedBlock::Table {
    headers: Vec<Vec<StyledSpan>>,
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<StyledSpan>>>,
}
```

### Parser state machine extension

New states:
```rust
ParserState::InTable { phase: TablePhase }

enum TablePhase {
    Head,
    HeadCell,
    Body,
    Row,
    Cell,
}
```

Events:
- `Event::Start(Tag::Table(alignments))` → begin table, save alignments
- `Event::Start(Tag::TableHead)` → enter head phase
- `Event::Start(Tag::TableCell)` → begin accumulating cell content (inline styled spans)
- `Event::End(TagEnd::TableCell)` → push cell to current row
- `Event::End(TagEnd::TableHead)` → save as headers
- `Event::Start(Tag::TableRow)` → begin new body row
- `Event::End(TagEnd::TableRow)` → push row
- `Event::End(TagEnd::Table)` → push completed `Table` block

---

## 3.4 — Layout: Lists and Block Quotes

### Lists

Each `ListItem` is flattened to `DocumentLine::Text` lines with:
- Indentation: `indent = 2 * depth` spaces prepended
- Prefix: bullet character or number (first line only)
- Child blocks: recursively flatten at `depth + 1`

```
• First item
  Continuation of first item (wrapped)
  ◦ Nested item
    ▪ Deeply nested
• Second item
```

Task list items:
```
☑ Completed task
☐ Pending task
```

### Block Quotes

Each child block is flattened recursively, then every resulting line is prefixed:
```
│ quoted text here
│ │ nested quote
```

Apply dimmed/italic modifier to the entire block quote text.

**Standards note:** The recursive flattening of lists-in-quotes and quotes-in-lists
is the most complex nesting case. Use a helper:
```rust
fn flatten_block(block: &RenderedBlock, width: u16, indent: u16, prefix: &str) -> Vec<DocumentLine>
```
This helper is called recursively for children. The `indent` and `prefix` parameters
accumulate as nesting deepens. (See [standards.md §3.5](standards.md) — Visitor Pattern)

---

## 3.5 — Layout: Tables

Column width calculation:
1. For each column: `max(header_width, max(cell_widths_in_column))`
2. If total width > terminal width: truncate widest columns proportionally

Emit:
1. Header row — `DocumentLine::Text` with bold styling, padded per column alignment
2. Separator — `DocumentLine::Text` with `─` repeated per column, `┼` at intersections
3. Data rows — `DocumentLine::Text` with alignment-based padding

Column alignment (from `pulldown_cmark::Alignment`):
- `Left`: pad right
- `Right`: pad left
- `Center`: pad both sides
- `None`: treat as left

---

## 3.6 — Renderer Updates

No new `DocumentLine` variants needed for lists and block quotes — they produce `Text`
lines with appropriate styling and indentation baked in.

Tables also produce `Text` lines with manually padded cells.

**Alternative:** If Ratatui's `Table` widget is a better fit, introduce a
`DocumentLine::TableRow` variant. Evaluate during implementation — the manual approach
is simpler and avoids widget-level scrolling complications.

---

## 3.7 — Test Data and Tests

### Test data files

**`testdata/lists.md`:**
- Unordered list (1 level)
- Ordered list starting at 1
- Ordered list starting at 5
- Nested list (3 levels)
- Mixed ordered/unordered nesting
- Task list with checked and unchecked items
- List items with multiple paragraphs
- List items containing code blocks

**`testdata/blockquotes.md`:**
- Single-level block quote
- Nested block quote (3 levels)
- Block quote containing a list
- Block quote containing a code block
- Block quote containing a heading

**`testdata/tables.md`:**
- Simple 3×3 table
- Table with left/center/right alignment
- Table with long cell content (wrapping)
- Table with many columns (width overflow)
- Table with empty cells
- Single-column table

### Unit tests

**`parser.rs`:**
- Unordered list → `List { ordered: false, items: [...] }`
- Ordered list starting at 3 → `List { ordered: true, start: Some(3), items: [...] }`
- Nested list → `ListItem.children` is non-empty
- Task list → `TaskListItem { checked: true/false }`
- Block quote → `BlockQuote { children: [...] }`
- Table → `Table { headers, rows, alignments }` with correct structure

**`layout.rs`:**
- Nested list → correct indentation in output lines
- Block quote → every line starts with `│ ` prefix
- Table → column widths match content

---

## Phase 3 — Definition of Done

- [ ] Unordered lists render with bullet characters (• ◦ ▪ by depth)
- [ ] Ordered lists render with correct numbering (including custom start)
- [ ] Nested lists (3+ levels) render with proper indentation
- [ ] Task lists render with ☑ / ☐ checkboxes
- [ ] List items with multiple paragraphs render correctly
- [ ] Block quotes render with │ prefix and italic/dimmed style
- [ ] Nested block quotes increase prefix depth
- [ ] Block quotes containing lists/code render correctly
- [ ] Tables render with aligned columns and styled headers
- [ ] Table column widths auto-calculate from content
- [ ] Tables wider than terminal are truncated (not panicking)
- [ ] All `match` arms updated for new `RenderedBlock` variants
- [ ] `cargo test` passes with all new tests
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Phase 1–2 features still work (no regressions)
- [ ] Phase gate checklist from [standards.md §10](standards.md) passes

**Files created/modified:**
- Created: `testdata/lists.md`, `testdata/blockquotes.md`, `testdata/tables.md`
- Modified: `src/parser.rs`, `src/layout.rs`, `src/renderer.rs`
