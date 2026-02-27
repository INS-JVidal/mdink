# mdink — Implementation Plan Overview

> Terminal markdown renderer in Rust, inspired by [charmbracelet/glow](https://github.com/charmbracelet/glow).
>
> **Pipeline:** `Markdown source → pulldown-cmark → RenderedBlock IR → DocumentLine → Ratatui TUI`
>
> **Distribution:** `curl | sh` installer + `apt install mdink` + `cargo install mdink`

---

## Plan Documents

| Document | Purpose |
|----------|---------|
| [standards.md](standards.md) | Code quality, architecture, SOLID, patterns, testing — applies to ALL phases |
| [initial_plan.md](initial_plan.md) | Original project vision, architecture, data types, reference projects |
| [phase1_minimal_renderer.md](phase1_minimal_renderer.md) | Project scaffold + basic text rendering + scrolling |
| [phase2_code_blocks.md](phase2_code_blocks.md) | Syntax-highlighted code blocks via syntect |
| [phase3_lists_quotes_tables.md](phase3_lists_quotes_tables.md) | Structured block elements with nesting |
| [phase4_images.md](phase4_images.md) | Terminal image support (Sixel/Kitty/iTerm2/halfblocks) |
| [phase5_theming.md](phase5_theming.md) | JSON theming system with 3 built-in themes |
| [phase6_polish.md](phase6_polish.md) | Links, footnotes, search, heading nav, pager mode |
| [phase7_packaging.md](phase7_packaging.md) | CI/CD, curl installer, .deb/apt, man pages, completions |

---

## Project Structure (final state)

```
mdink/
├── .github/
│   └── workflows/
│       ├── ci.yml                    # Build + test + clippy on every push/PR
│       └── release.yml               # Build binaries + .deb + publish on tag
├── packaging/
│   └── install.sh                    # curl installer script
├── assets/                           # Generated at build time (man page, completions)
│   ├── mdink.1.gz
│   └── completions/
│       ├── mdink.bash
│       ├── _mdink                    # zsh
│       └── mdink.fish
├── xtask/                            # cargo-xtask workspace member
│   ├── Cargo.toml
│   └── src/main.rs                   # dist-assets: generate man page + completions
├── Cargo.toml                        # Workspace root with [package.metadata.deb]
├── Cargo.lock                        # Committed for reproducible builds
├── LICENSE
├── src/
│   ├── main.rs                       # CLI entry point, wiring, event loop
│   ├── cli.rs                        # clap definition (separate for xtask reuse)
│   ├── app.rs                        # App state: scroll position, viewport, mode
│   ├── parser.rs                     # pulldown-cmark event stream → RenderedBlock
│   ├── renderer.rs                   # DocumentLine → Frame (ratatui rendering)
│   ├── highlight.rs                  # syntect integration + syntect→ratatui bridge
│   ├── images.rs                     # ratatui-image: load, cache, render images
│   ├── theme/
│   │   ├── mod.rs                    # Theme struct, loading, Style conversion
│   │   ├── dark.json                 # Built-in dark theme
│   │   ├── light.json                # Built-in light theme
│   │   └── dracula.json              # Built-in dracula theme
│   ├── layout.rs                     # Block measurement & vertical space allocation
│   └── keybindings.rs                # Input handling (vim-style + standard)
├── themes/
│   └── example-custom.json
├── testdata/
│   ├── basic.md
│   ├── code-blocks.md
│   ├── lists.md
│   ├── blockquotes.md
│   ├── tables.md
│   ├── images.md
│   └── full-featured.md
└── README.md
```

---

## Dependency Introduction Schedule

| Phase | New `Cargo.toml` additions |
|-------|---------------------------|
| 1 | `ratatui 0.30`, `pulldown-cmark 0.13`, `clap 4`, `unicode-width 0.2`, `textwrap 0.16`, `color-eyre 0.6` |
| 2 | `syntect 5.2` |
| 3 | *(none)* |
| 4 | `ratatui-image 10`, `image 0.25` |
| 5 | `serde 1`, `serde_json 1`, `dirs 5` |
| 6 | *(optional: `ureq 3` for URL fetching)* |
| 7 | `clap_mangen 0.2`, `clap_complete 4`, `flate2 1` (xtask only — not shipped in binary) |

---

## Cross-Phase Refactoring Notes

- **Phase 2 changes Phase 1 code:** `parser::parse()` gains a `&Highlighter` parameter. `RenderedBlock` grows a `CodeBlock` variant. `DocumentLine` gains a `Code` variant.
- **Phase 4 changes Phase 1–3 code:** `parser::parse()` gains a `&mut ImageManager` parameter. `DocumentLine` gains `ImageStart` and `ImageContinuation` variants. `renderer::draw()` needs `&mut App` access.
- **Phase 5 changes ALL prior code:** Every style-producing function takes `&MarkdownTheme`. This is the most invasive refactor.
- **Phase 7 requires `src/cli.rs` from Phase 1:** The xtask imports the `Cli` struct. This is why Phase 1 separates `cli.rs` from `main.rs`.
- **Recommendation:** Even during Phase 1, define a minimal `Theme` struct with hardcoded values so Phase 5 becomes "extend fields" rather than "thread a new parameter everywhere."

---

## Build and Release Matrix

| Artifact | Target | Linking | Used by |
|----------|--------|---------|---------|
| `mdink-v*-x86_64-unknown-linux-musl.tar.gz` | x86_64 Linux | static (musl) | curl installer |
| `mdink-v*-aarch64-unknown-linux-musl.tar.gz` | aarch64 Linux | static (musl) | curl installer |
| `mdink-v*-x86_64-apple-darwin.tar.gz` | x86_64 macOS | dynamic | curl installer |
| `mdink-v*-aarch64-apple-darwin.tar.gz` | aarch64 macOS | dynamic | curl installer |
| `mdink_{ver}_amd64.deb` | x86_64 Linux | dynamic (glibc) | apt |
| `mdink_{ver}_arm64.deb` | aarch64 Linux | dynamic (glibc) | apt |
| `checksums.txt` | all | — | checksum verification |

**Why two linking strategies:**
- **musl (static)** for curl installer — zero runtime dependencies, works on any Linux distro
- **glibc (dynamic)** for `.deb` — allows `$auto` dependency detection, expected by Debian policy

---

## Risk Register

| Risk | Impact | Mitigation |
|------|--------|-----------|
| ratatui-image v10 API differs from docs | Phase 4 blocked | Check crates.io/docs.rs for exact v10 API before coding |
| syntect load time is slow | Startup latency | Lazy-load or use `syntect::dumps` for pre-compiled syntax sets |
| Image rendering flickers on scroll | Bad UX | Use `StatefulImage`'s built-in resize caching |
| Textwrap doesn't handle styled text | Incorrect wrapping | Wrap on plain text, then re-apply styles to wrapped segments |
| pulldown-cmark 0.13 API changes | Parser broken | Pin exact version, check migration guide |
| Terminal doesn't support any graphics | Images don't render | Halfblocks fallback is universal; `ImageFallback` as last resort |
| musl build fails with C deps (image codecs) | No static binary | `actions-rust-cross` provides full musl sysroot; or `cargo-zigbuild` |
| `.deb` package missing runtime libs | Broken apt install | `$auto` depends + test in clean Docker container |
| GPG key compromise | Malicious packages | Dedicated CI-only signing key; rotate if compromised |
| GitHub Pages rate limits | apt repo unavailable | Static files — unlikely; mirror on Cloudsmith if needed |
| xtask can't import `cli.rs` | Can't generate assets | Keep `cli.rs` free of non-clap deps; use `#[path]` include |
| Apple Silicon cross-compile on GH Actions | No arm64 macOS binary | Use `macos-14` runner (native arm64) |
