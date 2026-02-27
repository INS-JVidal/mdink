//! CLI argument definition for mdink.
//!
//! This module is intentionally free of non-clap dependencies so that
//! the Phase 7 xtask can import it via `#[path]` for man page and
//! shell completion generation.

use clap::Parser;

/// Terminal markdown renderer.
#[derive(Parser)]
#[command(name = "mdink", version, about = "Terminal markdown renderer")]
pub struct Cli {
    /// Markdown file to render (use "-" for stdin).
    pub file: String,

    // Later phases will add: --style, --width, --pager, --no-images, --list-themes
}
