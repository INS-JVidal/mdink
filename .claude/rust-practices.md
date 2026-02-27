# Rust coding practices

Rules derived from real mistakes and refactors in this codebase.

## Match

**Put catch-all arms last within their group.**
Wildcards like `_` and `Type::Variant(_)` shadow every arm below them silently.
Always run `cargo test` with `-D warnings` — not just `cargo clippy` — so
`unreachable_patterns` is caught before tests are the first signal.

```rust
// Wrong — Start(_) swallows Link, Emphasis, Strong, etc.
Event::Start(_) => skip(),
Event::Start(Tag::Link { .. }) => handle_link(), // never reached

// Correct
Event::Start(Tag::Link { .. }) => handle_link(),
Event::Start(Tag::Emphasis) => handle_em(),
Event::Start(_) => skip(), // catch-all last
```

## Ownership & borrowing

**Copy cheap values out before mutating.**
When a shared borrow conflicts with a subsequent mutation, check whether the
needed value is `Copy` (or cheap to clone). Extracting it ends the borrow
before the mutation begins. Reach for `RefCell` only after this fails.

```rust
// depth is u32 (Copy) — extract it, release the borrow, then mutate freely
let depth = match self.state_stack.last() {
    Some(State::Skipping { depth }) => *depth,
    _ => return,
};
self.state_stack.pop();
self.state_stack.push(State::Skipping { depth: depth + 1 });
```

**Prefer `std::mem::take` over clone-then-clear.**
When a buffer is accumulated then consumed, `mem::take(&mut buf)` moves it
out and leaves an empty container in-place — one operation, no clone.

```rust
let content = std::mem::take(&mut self.current_spans); // drains, leaves vec![]
self.blocks.push(Block::Paragraph { content });
```

**Method extraction resolves borrow conflicts that comments explain away.**
If a function has a comment like "checked here to avoid overlapping borrows",
that is a signal the logic belongs in a method. Each method call gets its own
borrow scope; the conflict disappears without any unsafe or workaround.

## Error handling

**Match the tool to the failure domain.**

| Situation | Tool |
|-----------|------|
| Input might legitimately fail (user data, external API) | `.ok()?`, `?`, `unwrap_or` |
| Hardcoded constant that should never fail | explicit `match` + `debug_assert!` |
| Graceful degradation a user might encounter | `eprintln!` (or structured log) |
| Internal invariant that must hold if logic is correct | `debug_assert!` |

`.ok()?` on `Scope::new("comment")` silently disabled a feature when syntect
failed to initialise. An explicit match with `debug_assert!` would have caught
it in tests.

**`debug_assert!` expresses invariants, not degradation.**
It compiles to nothing in release — use it to document "this cannot happen if
my code is correct". Use logging or `eprintln!` for paths that could fire in
production and that users need to know about.

```rust
// Invariant: style stack must not be empty when popping inline format
fn pop_style(&mut self) {
    debug_assert!(!self.style_stack.is_empty(), "pop_style on empty style_stack");
    self.style_stack.pop();
}
```

## Testing

**Use `LazyLock` for expensive shared test fixtures.**
Pay the construction cost once per test binary, not once per test.
Applies to anything slow: loading syntax sets, spinning up a runtime,
parsing large schemas.

```rust
static HIGHLIGHTER: LazyLock<Highlighter> = LazyLock::new(Highlighter::new);

fn h() -> &'static Highlighter { &HIGHLIGHTER }
```

**Use `include_str!` for test fixtures that must exist.**
Embedding a file at compile time turns a missing or moved file into a build
error rather than a runtime test failure.

```rust
let source = include_str!("../testdata/font-slots.md");
```

## Structure

**Extract Stateful Object when a function accumulates multiple `mut` locals into a loop.**
The pattern is always the same:

```rust
// Before: 200+ line function with 4 mut locals
pub fn parse(src: &str) -> Vec<Block> {
    let mut blocks = vec![];
    let mut state_stack = ...;
    let mut style_stack = ...;
    let mut current_spans = ...;
    for event in parser { /* giant match */ }
    blocks
}

// After: consuming builder
struct ParseContext { blocks, state_stack, style_stack, current_spans }

impl ParseContext {
    fn new() -> Self { ... }
    fn process(mut self, src: &str) -> Vec<Block> { /* thin loop */ self.blocks }
    fn on_event(&mut self, e: Event) { /* routes to focused methods */ }
}

pub fn parse(src: &str) -> Vec<Block> {
    ParseContext::new().process(src)
}
```

Benefits: each handler is a named, independently testable method; borrow
conflicts resolve naturally; the public function is self-documenting.

## Architecture

**Enforce unidirectional data flow at module boundaries.**
Each stage imports only from earlier stages (or external crates). When this
holds, refactors stay local — changes to `parser.rs` touch zero lines in
`layout.rs` or `renderer.rs`.

**Hardcode size guards at system boundaries.**
Unbounded input to a CPU- or memory-intensive subsystem should be rejected
before processing begins. Put the guard at the callsite, not inside the library.

```rust
const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
if code.len() > MAX_HIGHLIGHT_BYTES {
    return plain_text_fallback(code);
}
```
