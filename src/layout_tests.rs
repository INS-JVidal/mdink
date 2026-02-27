    use super::*;
    use crate::parser::StyledSpan;
    use ratatui::style::{Color, Modifier, Style};

    fn plain_span(text: &str) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            style: Style::default(),
        }
    }

    fn styled_span(text: &str, style: Style) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            style,
        }
    }

    #[test]
    fn test_layout_empty_blocks() {
        let doc = flatten(&[], 80);
        assert_eq!(doc.total_height, 0);
        assert!(doc.lines.is_empty());
    }

    #[test]
    fn test_layout_single_paragraph_no_wrap() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("Hello world")],
        }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
        assert!(matches!(&doc.lines[0], DocumentLine::Text(_)));
    }

    #[test]
    fn test_layout_paragraph_wraps_at_width() {
        let long_text = "word ".repeat(20); // 100 chars
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span(long_text.trim())],
        }];
        let doc = flatten(&blocks, 40);
        assert!(
            doc.total_height > 1,
            "expected wrapping, got {} lines",
            doc.total_height
        );
    }

    #[test]
    fn test_layout_thematic_break() {
        let blocks = vec![RenderedBlock::ThematicBreak];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
        assert!(matches!(&doc.lines[0], DocumentLine::Rule));
    }

    #[test]
    fn test_layout_inter_block_spacing() {
        let blocks = vec![
            RenderedBlock::Paragraph {
                content: vec![plain_span("First")],
            },
            RenderedBlock::Paragraph {
                content: vec![plain_span("Second")],
            },
        ];
        let doc = flatten(&blocks, 80);
        // First paragraph (1 line) + empty (1 line) + second paragraph (1 line) = 3
        assert_eq!(doc.total_height, 3);
        assert!(matches!(&doc.lines[1], DocumentLine::Empty));
    }

    #[test]
    fn test_layout_heading_renders_as_text() {
        let blocks = vec![RenderedBlock::Heading {
            level: 1,
            content: vec![styled_span(
                "Title",
                Style::default().add_modifier(Modifier::BOLD),
            )],
        }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
        assert!(matches!(&doc.lines[0], DocumentLine::Text(_)));
    }

    #[test]
    fn test_layout_spacer() {
        let blocks = vec![RenderedBlock::Spacer { lines: 3 }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 3);
        for line in &doc.lines {
            assert!(matches!(line, DocumentLine::Empty));
        }
    }

    #[test]
    fn test_layout_single_long_word() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("abcdefghijklmnopqrstuvwxyz")],
        }];
        let doc = flatten(&blocks, 10);
        assert!(doc.total_height >= 2, "long word should wrap");
    }

    #[test]
    fn test_layout_empty_paragraph() {
        let blocks = vec![RenderedBlock::Paragraph { content: vec![] }];
        let doc = flatten(&blocks, 80);
        assert_eq!(doc.total_height, 1);
    }

    #[test]
    fn test_layout_preserves_styles_across_wrap() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let text = "word ".repeat(20);
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![styled_span(text.trim(), bold)],
        }];
        let doc = flatten(&blocks, 40);
        for line in &doc.lines {
            if let DocumentLine::Text(l) = line {
                for span in &l.spans {
                    assert!(
                        span.style.add_modifier.contains(Modifier::BOLD),
                        "style lost after wrapping"
                    );
                }
            }
        }
    }

    #[test]
    fn test_layout_repeated_text_no_misalignment() {
        // Regression test: repeated text must not confuse the cursor.
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("aaa bbb aaa bbb aaa bbb")],
        }];
        let doc = flatten(&blocks, 8);
        // Collect all text from the wrapped lines.
        let mut all_text = String::new();
        for line in &doc.lines {
            if let DocumentLine::Text(l) = line {
                for span in &l.spans {
                    all_text.push_str(&span.content);
                }
                all_text.push(' '); // represent line break as space
            }
        }
        // All original words must appear (no duplication, no loss).
        assert_eq!(all_text.matches("aaa").count(), 3, "word 'aaa' count");
        assert_eq!(all_text.matches("bbb").count(), 3, "word 'bbb' count");
    }

    #[test]
    fn test_layout_multi_style_spans_across_wrap() {
        // Two styled spans that together exceed the width.
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let italic = Style::default().add_modifier(Modifier::ITALIC);
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![
                styled_span("hello ", bold),
                styled_span("world this is long", italic),
            ],
        }];
        let doc = flatten(&blocks, 12);
        assert!(doc.total_height >= 2, "should wrap");
        // First line should have bold "hello " and italic "world"
        if let DocumentLine::Text(first_line) = &doc.lines[0] {
            assert!(!first_line.spans.is_empty(), "first line should have spans");
        }
    }

    #[test]
    fn test_layout_unicode_emoji_no_panic() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("Hello 🌍 world 🎉 test 🚀 more text here for wrapping")],
        }];
        // Should not panic on emoji at any width.
        let doc = flatten(&blocks, 15);
        assert!(doc.total_height >= 1);
    }

    #[test]
    fn test_layout_cjk_text_no_panic() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("日本語のテキスト処理テスト")],
        }];
        let doc = flatten(&blocks, 10);
        assert!(doc.total_height >= 1);
    }

    #[test]
    fn test_layout_zero_width_no_panic() {
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![plain_span("text")],
        }];
        // Width 0 is clamped to 1 — should not panic.
        let doc = flatten(&blocks, 0);
        assert!(doc.total_height >= 1);
    }

    #[test]
    fn test_layout_mixed_styles_content_preserved() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let code = Style::default().fg(Color::Indexed(252)).bg(Color::Indexed(236));
        let blocks = vec![RenderedBlock::Paragraph {
            content: vec![
                styled_span("Use ", Style::default()),
                styled_span("fmt", code),
                styled_span(" for formatting output in your programs", bold),
            ],
        }];
        let doc = flatten(&blocks, 20);
        // Collect all text.
        let mut all_text = String::new();
        for line in &doc.lines {
            if let DocumentLine::Text(l) = line {
                for span in &l.spans {
                    all_text.push_str(&span.content);
                }
            }
        }
        assert!(all_text.contains("Use "), "should contain 'Use '");
        assert!(all_text.contains("fmt"), "should contain 'fmt'");
        assert!(
            all_text.contains("formatting"),
            "should contain 'formatting'"
        );
    }

    // ── Phase 2: Code block layout tests ────────────────────────

    fn make_code_line(text: &str) -> Line<'static> {
        Line::from(Span::raw(text.to_string()))
    }

    #[test]
    fn test_layout_code_block_long_line_no_wrap() {
        let long_line = "x".repeat(200);
        let blocks = vec![RenderedBlock::CodeBlock {
            language: String::new(),
            highlighted_lines: vec![make_code_line(&long_line)],
        }];
        let doc = flatten(&blocks, 40);
        // Code lines should NOT wrap — still 1 Code line.
        let code_count = doc
            .lines
            .iter()
            .filter(|l| matches!(l, DocumentLine::Code(_)))
            .count();
        assert_eq!(code_count, 1, "code should not wrap");
    }

    #[test]
    fn test_layout_code_block_empty_language_no_label() {
        let blocks = vec![RenderedBlock::CodeBlock {
            language: String::new(),
            highlighted_lines: vec![make_code_line("code")],
        }];
        let doc = flatten(&blocks, 80);
        // No language → no label line, just the code line.
        assert_eq!(doc.total_height, 1);
    }

    #[test]
    fn test_layout_code_block_with_language_has_label() {
        let blocks = vec![RenderedBlock::CodeBlock {
            language: "rust".to_string(),
            highlighted_lines: vec![
                make_code_line("fn main() {"),
                make_code_line("    println!(\"hello\");"),
                make_code_line("}"),
            ],
        }];
        let doc = flatten(&blocks, 80);
        // 1 label + 3 code lines = 4
        assert_eq!(doc.total_height, 4);
        // First line should be the label.
        if let DocumentLine::Code(label_line) = &doc.lines[0] {
            let text: String = label_line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains("rust"), "label should contain language name");
        } else {
            panic!("expected Code line for label");
        }
    }

    #[test]
    fn test_layout_code_block_multiple_lines_correct_count() {
        let blocks = vec![RenderedBlock::CodeBlock {
            language: "python".to_string(),
            highlighted_lines: vec![
                make_code_line("def f():"),
                make_code_line("    pass"),
            ],
        }];
        let doc = flatten(&blocks, 80);
        // 1 label + 2 code lines = 3
        let code_count = doc
            .lines
            .iter()
            .filter(|l| matches!(l, DocumentLine::Code(_)))
            .count();
        assert_eq!(code_count, 3);
    }
