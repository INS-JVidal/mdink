//! Syntax highlighting bridge: wraps syntect behind a single `Highlighter` type.
//!
//! This is a **leaf module** — it never imports from other mdink modules.
//! The rest of the codebase interacts with syntax highlighting exclusively
//! through `Highlighter::highlight_code()`, which returns `Vec<Line<'static>>`.
//!
//! This isolation means `syntect` types never leak into the parser, layout,
//! or renderer — Dependency Inversion per standards §2.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, ThemeSet};
use syntect::parsing::{Scope, SyntaxSet};
use syntect::util::LinesWithEndings;

/// Wraps syntect's syntax and theme sets, loaded once at startup.
///
/// `SyntaxSet` and `ThemeSet` are expensive to construct (~50ms each).
/// This struct ensures they are loaded once and reused for every code block.
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

/// Default syntect theme used for code highlighting.
const DEFAULT_THEME: &str = "base16-ocean.dark";

impl Highlighter {
    /// Creates a new `Highlighter` with default syntax and theme sets.
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Highlights a code block, returning one `Line<'static>` per source line.
    ///
    /// - `language` is matched via `find_syntax_by_token` (e.g. "rust", "py", "js").
    ///   Falls back to plain text if the language is unknown or empty.
    /// - `theme_name` selects a syntect built-in theme. Falls back to
    ///   `"base16-ocean.dark"` if not found.
    /// - Trailing newlines are stripped from each span (ratatui uses separate
    ///   `Line` objects, not embedded newlines).
    pub fn highlight_code(
        &self,
        code: &str,
        language: &str,
        theme_name: &str,
    ) -> Vec<Line<'static>> {
        // Guard against unbounded memory/CPU: Oniguruma (syntect's regex engine) can
        // exhaust memory on large inputs, surfacing as a panic rather than an Err.
        // Blocks exceeding the limit are rendered as plain unstyled text instead.
        const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024; // 512 KB
        if code.len() > MAX_HIGHLIGHT_BYTES {
            return code
                .lines()
                .map(|l| Line::from(Span::raw(l.to_string())))
                .collect();
        }

        let syntax = if language.is_empty() {
            self.syntax_set.find_syntax_plain_text()
        } else {
            self.syntax_set
                .find_syntax_by_token(language)
                .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
        };

        let Some(theme) = self
            .theme_set
            .themes
            .get(theme_name)
            .or_else(|| self.theme_set.themes.get(DEFAULT_THEME))
        else {
            // No theme available at all (e.g. syntect built without default themes).
            // Fall back to unstyled plain text so the app never panics at startup.
            return code
                .lines()
                .map(|l| Line::from(Span::raw(l.to_string())))
                .collect();
        };

        let comment_color = resolve_comment_color(theme);
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in LinesWithEndings::from(code) {
            let Ok(ranges) = highlighter.highlight_line(line, &self.syntax_set) else {
                // On highlight failure, emit the raw line as plain text.
                // Strip both \n and \r\n — LinesWithEndings includes the line ending.
                result.push(Line::from(Span::raw(
                    line.trim_end_matches(['\r', '\n']).to_string(),
                )));
                continue;
            };

            let spans: Vec<Span<'static>> = ranges
                .iter()
                .map(|(style, text)| {
                    // Strip \r\n, not just \n, for files with Windows line endings.
                    let trimmed = text.trim_end_matches(['\r', '\n']);
                    let is_comment =
                        comment_color.is_some_and(|cc| style.foreground == cc);
                    syntect_style_to_span(trimmed, *style, is_comment)
                })
                .filter(|span| !span.content.is_empty())
                .collect();

            result.push(Line::from(spans));
        }

        result
    }
}

/// Resolves the foreground color that the given theme assigns to the `comment` scope.
///
/// Returns `None` if the scope can't be parsed or the theme doesn't assign
/// a distinct color to comments (i.e. it matches the default foreground).
fn resolve_comment_color(
    theme: &syntect::highlighting::Theme,
) -> Option<syntect::highlighting::Color> {
    let comment_scope = match Scope::new("comment") {
        Ok(s) => s,
        Err(_) => {
            debug_assert!(false, "failed to parse hardcoded 'comment' scope");
            return None;
        }
    };
    let highlighter = syntect::highlighting::Highlighter::new(theme);
    let style = highlighter.style_for_stack(&[comment_scope]);
    let default_fg = theme
        .settings
        .foreground
        .unwrap_or(syntect::highlighting::Color::BLACK);
    if style.foreground == default_fg {
        None
    } else {
        Some(style.foreground)
    }
}

/// Converts a syntect highlighted segment into a ratatui `Span`.
///
/// Maps syntect RGB colors → `Color::Rgb` and syntect `FontStyle` flags
/// → ratatui `Modifier` flags. When `is_comment` is true, `ITALIC` is
/// forced so that comments land in the italic font slot.
/// Returns `Span<'static>` because we call `to_string()` to create owned data.
fn syntect_style_to_span(text: &str, style: SyntectStyle, is_comment: bool) -> Span<'static> {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    let bg = Color::Rgb(style.background.r, style.background.g, style.background.b);

    let mut modifier = Modifier::empty();
    if style.font_style.contains(FontStyle::BOLD) {
        modifier |= Modifier::BOLD;
    }
    if style.font_style.contains(FontStyle::ITALIC) || is_comment {
        modifier |= Modifier::ITALIC;
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        modifier |= Modifier::UNDERLINED;
    }

    Span::styled(
        text.to_string(),
        Style::default().fg(fg).bg(bg).add_modifier(modifier),
    )
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
