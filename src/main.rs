//! Entry point: wires CLI → parser → layout → TUI event loop.
//!
//! This is the thin orchestrator that connects all pipeline stages.
//! It handles CLI argument parsing, file I/O, terminal initialization,
//! the event loop, and graceful shutdown.

mod app;
mod cli;
mod layout;
mod parser;
mod renderer;

use std::fs;

use clap::Parser;
use ratatui::crossterm::event::{self, Event};

use crate::app::App;
use crate::cli::Cli;

fn main() -> color_eyre::Result<()> {
    // Install color_eyre error/panic hooks for pretty backtraces.
    color_eyre::install()?;

    // Chain our panic hook to restore the terminal before printing the backtrace.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        original_hook(info);
    }));

    // Parse CLI arguments.
    let cli = Cli::parse();

    // Read the markdown source file.
    let source = fs::read_to_string(&cli.file)?;

    // Parse markdown into IR blocks.
    let blocks = parser::parse(&source);

    // Get initial terminal size for layout.
    let (cols, _rows) = ratatui::crossterm::terminal::size()?;

    // Flatten blocks into document lines at the current width.
    let document = layout::flatten(&blocks, cols);

    // Create the application state.
    let mut app = App::new(document, cli.file.clone());

    // Initialize the terminal (enters raw mode + alternate screen).
    let mut terminal = ratatui::init();

    // Main event loop.
    let result = run_event_loop(&mut terminal, &mut app, &source);

    // Always restore the terminal, even if the loop returned an error.
    ratatui::restore();

    result
}

/// Runs the TUI event loop until the user quits or an error occurs.
///
/// Separated from `main()` so that `ratatui::restore()` always runs
/// regardless of how this function exits.
fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    source: &str,
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
                // Re-flatten the document at the new terminal width.
                let blocks = parser::parse(source);
                app.document = layout::flatten(&blocks, cols);
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
