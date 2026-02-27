# Phase 4: Terminal Image Support

> **Prerequisites:** Phase 3 complete
> **Standards:** All code must follow [standards.md](standards.md)
> **New dependencies:** `ratatui-image = { version = "10", default-features = false, features = ["image-defaults", "crossterm"] }`, `image = "0.25"`

**Goal:** Render inline images via terminal graphics protocols (Sixel, Kitty, iTerm2,
halfblocks) with graceful fallback when unsupported.

---

## 4.1 — Image Module (`src/images.rs`)

A **leaf module** — no imports from other mdink modules.
(See [standards.md §1.2](standards.md))

### `ImageManager` struct

```rust
pub struct ImageManager {
    picker: Option<Picker>,
    protocols: Vec<StatefulProtocol>,
    base_path: PathBuf,
}
```

- `picker: Option<Picker>` — `None` if terminal doesn't support any graphics protocol
- `protocols: Vec<StatefulProtocol>` — arena storage for loaded image protocols
- `base_path: PathBuf` — directory of the markdown file (for resolving relative paths)

### Methods

```rust
impl ImageManager {
    /// Create a new ImageManager. Queries the terminal for graphics support.
    /// If the query fails, images will use fallback mode.
    pub fn new(base_path: PathBuf) -> Self

    /// Load an image and return its index in the protocols vec.
    /// Returns Err if the image can't be loaded (file not found, decode error, no picker).
    pub fn load_image(&mut self, src: &str, max_width: u16) -> Result<(usize, u16, u16)>
    //                                                           index, width, height

    /// Get a mutable reference to a protocol by index.
    /// Used by the renderer at draw time.
    pub fn get_protocol(&mut self, index: usize) -> &mut StatefulProtocol
}
```

### `new()` implementation

```rust
pub fn new(base_path: PathBuf) -> Self {
    // Picker::from_query_stdio() queries the terminal for Sixel/Kitty/iTerm2 support.
    // If it fails, we set picker to None (fallback to alt text).
    let picker = Picker::from_query_stdio().ok();
    Self { picker, protocols: Vec::new(), base_path }
}
```

### `load_image()` implementation

1. Resolve `src` path relative to `base_path`
2. Open with `image::ImageReader::open(path)?.decode()?`
3. Scale to fit `max_width` cells (maintaining aspect ratio)
4. Call `picker.as_mut()?.new_resize_protocol(dyn_img)` → push to `protocols`
5. Return `(index, width_cells, height_cells)`

**Standards note:** All errors are `Result`-based, never panic. Missing images degrade
to alt text display. (See [standards.md §4.3](standards.md) — Graceful Degradation)

---

## 4.2 — Parser: Image Events

### New IR variants

```rust
RenderedBlock::Image {
    protocol_index: usize,
    alt_text: String,
    width_cells: u16,
    height_cells: u16,
}

RenderedBlock::ImageFallback {
    alt_text: String,
}
```

### Parser state machine extension

New state:
```rust
ParserState::InImage { dest_url: String, alt_buffer: String }
```

Events:
- `Event::Start(Tag::Image { dest_url, title, .. })` → enter `InImage` state, save URL
- `Event::Text(alt_text)` while in `InImage` → accumulate into alt buffer
- `Event::End(TagEnd::Image)` → attempt `image_manager.load_image(dest_url, max_width)`:
  - Success → push `Image { protocol_index, alt_text, width, height }`
  - Failure → push `ImageFallback { alt_text }`

### Signature change

```rust
// Phase 2:
pub fn parse(source: &str, highlighter: &Highlighter) -> Vec<RenderedBlock>

// Phase 4:
pub fn parse(source: &str, highlighter: &Highlighter, image_manager: &mut ImageManager) -> Vec<RenderedBlock>
```

---

## 4.3 — Layout: Images

### New `DocumentLine` variants

```rust
DocumentLine::ImageStart { protocol_index: usize, height: u16 }
DocumentLine::ImageContinuation
```

### Flattening

For `RenderedBlock::Image`:
- Emit `DocumentLine::ImageStart { protocol_index, height }`
- Emit `(height - 1)` × `DocumentLine::ImageContinuation` lines
- These placeholder lines reserve the correct vertical space for scrolling

For `RenderedBlock::ImageFallback`:
- Emit `DocumentLine::Text(Line::from(format!("[image: {}]", alt_text)))` with styled alt text

**Standards note:** The index-based indirection pattern avoids borrow-checker conflicts.
`DocumentLine` stores a `usize` index, not the `StatefulProtocol` itself.
(See [standards.md §3.4](standards.md))

---

## 4.4 — Renderer: Images

In `draw()`, when encountering `ImageStart { protocol_index, height }`:

1. Calculate the render area: `Rect { x, y, width: terminal_width, height }`
2. Get `&mut StatefulProtocol` from `app.image_manager.get_protocol(protocol_index)`
3. Render: `frame.render_stateful_widget(StatefulImage::default(), area, protocol)`
4. Skip the next `height - 1` `ImageContinuation` lines in the iteration

### Mutability requirement

`StatefulProtocol` needs `&mut` access at render time. This changes the renderer signature:

```rust
// Phase 1-3:
pub fn draw(frame: &mut Frame, app: &App)

// Phase 4:
pub fn draw(frame: &mut Frame, app: &mut App)
// (or split: &App for state, &mut ImageManager for images)
```

**Prefer the split approach** to maintain the principle that the renderer doesn't
modify application state:
```rust
pub fn draw(frame: &mut Frame, app: &App, image_manager: &mut ImageManager)
```

---

## 4.5 — CLI Flag: `--no-images`

Add to `src/cli.rs`:
```rust
/// Disable image rendering (show alt text instead)
#[arg(long)]
pub no_images: bool,
```

When set: skip `ImageManager::new()` terminal query and treat all images as `ImageFallback`.

---

## 4.6 — Test Data and Tests

### Test data

**`testdata/images.md`:**
```markdown
# Image Tests

A local image:
![Test image](test-image.png)

A missing image:
![Missing](nonexistent.png)

An image with no alt text:
![]( test-image.png)
```

Add a small test PNG to `testdata/test-image.png` (e.g., 100×100 solid color).

### Unit tests

**`images.rs`:**
- `load_image` with valid path → returns `Ok((index, width, height))`
- `load_image` with missing path → returns `Err`
- `load_image` with unsupported format → returns `Err`
- `get_protocol` with valid index → returns reference

**`parser.rs`:**
- Image tag with loadable image → `Image` variant
- Image tag with missing file → `ImageFallback` variant
- Image inside a paragraph → handled correctly

**Integration tests:**
- Render `testdata/images.md` without panic (image loading may fail in CI — fallback must work)

---

## Phase 4 — Definition of Done

- [ ] Images render via terminal graphics protocol (Sixel/Kitty/iTerm2/halfblocks)
- [ ] Terminal protocol auto-detected at startup via `Picker::from_query_stdio()`
- [ ] Images scale to fit terminal width (maintaining aspect ratio)
- [ ] Missing/broken images gracefully fall back to `[image: alt text]`
- [ ] `--no-images` flag disables image rendering entirely
- [ ] Image paths resolve relative to the markdown file's directory
- [ ] Scrolling works correctly past images (correct height reservation)
- [ ] No crashes on any terminal (graceful degradation)
- [ ] `ImageManager` is a leaf module (no mdink-internal imports)
- [ ] Renderer doesn't modify `App` state (split signature)
- [ ] All `match` arms updated for new `Image`/`ImageFallback`/`ImageStart`/`ImageContinuation` variants
- [ ] `cargo test` passes with all new tests
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Phase 1–3 features still work (no regressions)
- [ ] Phase gate checklist from [standards.md §10](standards.md) passes

**Files created/modified:**
- Created: `src/images.rs`, `testdata/images.md`, `testdata/test-image.png`
- Modified: `Cargo.toml` (uncomment ratatui-image, image), `src/parser.rs`, `src/layout.rs`, `src/renderer.rs`, `src/main.rs`, `src/cli.rs`
