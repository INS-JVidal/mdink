//! Entry point: wires CLI → parser → layout → TUI event loop.
//!
//! This is the thin orchestrator that connects all pipeline stages.
//! It handles CLI argument parsing, file I/O, terminal initialization,
//! the event loop, and graceful shutdown.

mod app;
mod cli;
mod highlight;
mod layout;
mod parser;
mod renderer;

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use ratatui::crossterm::event::{self, Event};

use crate::app::App;
use crate::cli::Cli;
use crate::parser::RenderedBlock;

/// Set to `true` immediately after `ratatui::init()` so the panic hook knows
/// whether the terminal has been initialised and needs restoring.
///
/// Calling `ratatui::restore()` before `ratatui::init()` sends spurious
/// escape sequences to the terminal, which can corrupt the calling shell's
/// display on some terminals and multiplexers.
static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);

fn main() -> color_eyre::Result<()> {
    // Install color_eyre error/panic hooks for pretty backtraces.
    color_eyre::install()?;

    // Chain our panic hook to restore the terminal before printing the backtrace.
    // The restore is guarded by TERMINAL_ACTIVE so it only runs after ratatui::init().
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if TERMINAL_ACTIVE.load(Ordering::SeqCst) {
            ratatui::restore();
        }
        original_hook(info);
    }));

    // Parse CLI arguments.
    let cli = Cli::parse();

    // Guard against OOM: reject files that exceed a reasonable size threshold.
    // The check happens before ratatui::init() so the error prints to the normal
    // terminal instead of a raw alternate screen.
    const MAX_FILE_BYTES: u64 = 100 * 1024 * 1024; // 100 MB
    let file_size = fs::metadata(&cli.file)?.len();
    if file_size > MAX_FILE_BYTES {
        return Err(color_eyre::eyre::eyre!(
            "{}: file too large ({} bytes; limit is {} bytes)",
            cli.file,
            file_size,
            MAX_FILE_BYTES
        ));
    }

    // Read the markdown source file.
    let source = fs::read_to_string(&cli.file)?;

    // Load syntax highlighting resources (expensive, done once).
    let highlighter = highlight::Highlighter::new();

    // Parse markdown into IR blocks (done once — blocks don't depend on width).
    let blocks = parser::parse(&source, &highlighter);

    // Get initial terminal size for layout.
    let (cols, _rows) = ratatui::crossterm::terminal::size()?;

    // Flatten blocks into document lines at the current width.
    let document = layout::flatten(&blocks, cols);

    // Sanitize filename for display: strip control characters and ANSI escape
    // sequences so a crafted filename cannot inject terminal escape codes into
    // the status bar output.
    let safe_filename = cli
        .file
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();

    // Create the application state.
    let mut app = App::new(document, safe_filename);

    // Initialize the terminal (enters raw mode + alternate screen).
    // TERMINAL_ACTIVE must be set immediately after so the panic hook is correct.
    let mut terminal = ratatui::init();
    TERMINAL_ACTIVE.store(true, Ordering::SeqCst);

    // Main event loop.
    let result = run_event_loop(&mut terminal, &mut app, &blocks);

    // Always restore the terminal, even if the loop returned an error.
    ratatui::restore();

    result
}

/// Runs the TUI event loop until the user quits or an error occurs.
///
/// Separated from `main()` so that `ratatui::restore()` always runs
/// regardless of how this function exits. Takes a reference to the
/// parsed blocks so resize can re-flatten without re-parsing.
fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    blocks: &[RenderedBlock],
) -> color_eyre::Result<()> {
    loop {
        // Update viewport height from current terminal size.
        app.viewport_height = terminal.size()?.height.saturating_sub(1) as usize;

        // Draw the current frame.
        terminal.draw(|frame| renderer::draw(frame, app))?;

        // Block until the next event.
        let event = event::read()?;

        match event {
            Event::Key(key) => {
                app.handle_key(key);
            }
            Event::Resize(cols, _rows) => {
                // Re-flatten at the new width (blocks are unchanged).
                app.document = layout::flatten(blocks, cols);
                // Clamp scroll offset to the new max.
                let max = app.max_scroll();
                if app.scroll_offset > max {
                    app.scroll_offset = max;
                }
            }
            // Ignore mouse, focus, and paste events.
            _ => {}
        }

        if app.quit {
            break;
        }
    }

    Ok(())
}
