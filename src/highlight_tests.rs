    use super::*;

    fn highlighter() -> &'static Highlighter {
        use std::sync::LazyLock;
        static H: LazyLock<Highlighter> = LazyLock::new(Highlighter::new);
        &H
    }

    #[test]
    fn test_highlight_known_rust_code() {
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let lines = highlighter().highlight_code(code, "rust", DEFAULT_THEME);
        assert!(!lines.is_empty(), "should produce highlighted lines");
        // Rust code should have colored spans (not all default style).
        let has_color = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.style.fg != Some(Color::default()))
        });
        assert!(has_color, "Rust code should have syntax coloring");
    }

    #[test]
    fn test_highlight_unknown_language_no_crash() {
        let code = "some random text\nmore text\n";
        let lines = highlighter().highlight_code(code, "nosuchlanguage", DEFAULT_THEME);
        assert!(!lines.is_empty(), "should produce lines even for unknown lang");
    }

    #[test]
    fn test_highlight_empty_code() {
        let lines = highlighter().highlight_code("", "rust", DEFAULT_THEME);
        assert!(lines.is_empty(), "empty code should produce no lines");
    }

    #[test]
    fn test_highlight_empty_language() {
        let code = "plain text\n";
        let lines = highlighter().highlight_code(code, "", DEFAULT_THEME);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_highlight_invalid_theme_falls_back() {
        let code = "fn main() {}\n";
        let lines = highlighter().highlight_code(code, "rust", "nonexistent-theme");
        assert!(!lines.is_empty(), "should fall back to default theme");
    }

    #[test]
    fn test_highlight_no_trailing_newlines_in_spans() {
        let code = "line one\nline two\n";
        let lines = highlighter().highlight_code(code, "rust", DEFAULT_THEME);
        for line in &lines {
            for span in &line.spans {
                assert!(
                    !span.content.ends_with('\n'),
                    "span should not have trailing newline: {:?}",
                    span.content
                );
                assert!(
                    !span.content.ends_with('\r'),
                    "span should not have trailing CR (CRLF): {:?}",
                    span.content
                );
            }
        }
    }

    #[test]
    fn test_highlight_crlf_line_endings_no_cr_in_spans() {
        // Simulate a file with Windows-style CRLF line endings in code content.
        let code = "fn main() {\r\n    let x = 1;\r\n}\r\n";
        let lines = highlighter().highlight_code(code, "rust", DEFAULT_THEME);
        assert!(!lines.is_empty());
        for line in &lines {
            for span in &line.spans {
                assert!(
                    !span.content.contains('\r'),
                    "span should not contain CR character: {:?}",
                    span.content
                );
            }
        }
    }

    #[test]
    fn test_highlight_non_ascii_code_no_panic() {
        // Unicode characters in comments and strings are common in real code.
        let code = "// Arrow → and ellipsis …\nlet s = \"héllo wörld\";\n";
        let lines = highlighter().highlight_code(code, "rust", DEFAULT_THEME);
        assert_eq!(lines.len(), 2, "non-ASCII code should produce correct line count");
        // Verify no trailing newlines or CRs.
        for line in &lines {
            for span in &line.spans {
                assert!(!span.content.ends_with('\n'));
                assert!(!span.content.ends_with('\r'));
            }
        }
    }

    #[test]
    fn test_highlight_python_code() {
        let code = "def hello():\n    print(\"world\")\n";
        let lines = highlighter().highlight_code(code, "python", DEFAULT_THEME);
        assert_eq!(lines.len(), 2);
    }

    // ── Font slot strategy tests ────────────────────────────────

    #[test]
    fn test_resolve_comment_color_base16_ocean() {
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get("base16-ocean.dark")
            .expect("base16-ocean.dark must be a built-in syntect theme");
        let color = resolve_comment_color(theme);
        assert!(color.is_some(), "base16-ocean.dark should have a comment color");
        let c = color.expect("color must be Some after is_some() assertion");
        // base16-ocean.dark comment color is #65737e → (101, 115, 126)
        assert_eq!(
            (c.r, c.g, c.b),
            (101, 115, 126),
            "expected base16-ocean.dark comment color"
        );
    }

    #[test]
    fn test_highlight_comment_gets_italic() {
        let code = "// this is a comment\n";
        let lines = highlighter().highlight_code(code, "rust", DEFAULT_THEME);
        assert_eq!(lines.len(), 1);
        let has_italic = lines[0]
            .spans
            .iter()
            .any(|span| span.style.add_modifier.contains(Modifier::ITALIC));
        assert!(has_italic, "comment spans should have ITALIC modifier");
    }

    #[test]
    fn test_highlight_non_comment_no_forced_italic() {
        let code = "let x = 42;\n";
        let lines = highlighter().highlight_code(code, "rust", DEFAULT_THEME);
        assert_eq!(lines.len(), 1);
        // None of the spans in a simple assignment should be forced italic
        // (unless syntect's theme itself marks them italic, which base16-ocean doesn't).
        let all_non_italic = lines[0]
            .spans
            .iter()
            .all(|span| !span.style.add_modifier.contains(Modifier::ITALIC));
        assert!(
            all_non_italic,
            "non-comment code should not have forced ITALIC"
        );
    }
