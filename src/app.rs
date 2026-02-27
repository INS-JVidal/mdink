//! Application state: scroll position, viewport dimensions, quit flag.
//!
//! `App` is a pure state container — it never imports `ratatui::Frame` or
//! performs any rendering. The renderer reads from `&App` to determine
//! what to draw.

use std::ops::Range;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::layout::PreRenderedDocument;

/// Application state for the TUI viewer.
///
/// Holds the pre-rendered document, scroll position, viewport size,
/// and session metadata. Methods handle keyboard input and scroll
/// arithmetic.
pub struct App {
    /// The pre-rendered document (all lines laid out for display).
    pub document: PreRenderedDocument,
    /// Current vertical scroll offset (0 = top of document).
    pub scroll_offset: usize,
    /// Number of visible lines in the content area (excludes status bar).
    pub viewport_height: usize,
    /// Name of the file being displayed (shown in the status bar).
    pub filename: String,
    /// When true, the event loop should exit.
    pub quit: bool,
}

impl App {
    /// Creates a new `App` with the given document and filename.
    ///
    /// Scroll starts at the top; viewport height is set to 0 and must
    /// be updated by `main.rs` before each draw call.
    pub fn new(document: PreRenderedDocument, filename: String) -> Self {
        Self {
            document,
            scroll_offset: 0,
            viewport_height: 0,
            filename,
            quit: false,
        }
    }

    /// Dispatches a key event to the appropriate scroll or quit action.
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            // Scroll down 1 line
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(1),
            // Scroll up 1 line
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(1),
            // Scroll down half-page
            KeyCode::Char('d') | KeyCode::PageDown => {
                let half = self.viewport_height / 2;
                self.scroll_down(half.max(1));
            }
            // Scroll up half-page
            KeyCode::Char('u') | KeyCode::PageUp => {
                let half = self.viewport_height / 2;
                self.scroll_up(half.max(1));
            }
            // Scroll to top
            KeyCode::Char('g') | KeyCode::Home => self.scroll_to_top(),
            // Scroll to bottom (Shift+g = 'G')
            KeyCode::Char('G') | KeyCode::End => self.scroll_to_bottom(),
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            // Ctrl+C also quits
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
            }
            _ => {}
        }
    }

    /// Returns the range of line indices visible in the current viewport.
    pub fn visible_range(&self) -> Range<usize> {
        let end = (self.scroll_offset + self.viewport_height).min(self.document.total_height);
        self.scroll_offset..end
    }

    /// Scrolls down by `n` lines, clamped to the maximum scroll position.
    pub fn scroll_down(&mut self, n: usize) {
        let max = self.max_scroll();
        // Use saturating_add so overflow before .min() cannot wrap to a small value.
        // (scroll_up already uses saturating_sub symmetrically.)
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max);
    }

    /// Scrolls up by `n` lines, clamped to 0.
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scrolls to the top of the document.
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scrolls to the bottom of the document.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
    }

    /// Returns the maximum valid scroll offset.
    ///
    /// When the document is shorter than the viewport, returns 0 (no scrolling).
    pub fn max_scroll(&self) -> usize {
        self.document
            .total_height
            .saturating_sub(self.viewport_height)
    }

    /// Returns the current scroll position as a percentage (0–100).
    ///
    /// Returns 100 when the document fits within the viewport or when
    /// scrolled to the bottom.
    pub fn scroll_percent(&self) -> u16 {
        let max = self.max_scroll();
        if max == 0 {
            return 100;
        }
        ((self.scroll_offset as f64 / max as f64) * 100.0) as u16
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
