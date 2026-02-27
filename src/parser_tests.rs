    use super::*;
    use std::sync::LazyLock;

    static TEST_HIGHLIGHTER: LazyLock<crate::highlight::Highlighter> =
        LazyLock::new(crate::highlight::Highlighter::new);

    fn h() -> &'static crate::highlight::Highlighter {
        &TEST_HIGHLIGHTER
    }

    #[test]
    fn test_parser_heading_h1_produces_heading_block() {
        let blocks = parse("# Hello", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Heading { level, content } => {
                assert_eq!(*level, 1);
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "Hello");
            }
            _ => panic!("expected Heading block"),
        }
    }

    #[test]
    fn test_parser_heading_all_levels() {
        for lvl in 1..=6 {
            let md = format!("{} Level {}", "#".repeat(lvl), lvl);
            let blocks = parse(&md, h());
            assert_eq!(blocks.len(), 1, "level {lvl}");
            match &blocks[0] {
                RenderedBlock::Heading { level, .. } => {
                    assert_eq!(*level, lvl as u8, "level {lvl}");
                }
                _ => panic!("expected Heading at level {lvl}"),
            }
        }
    }

    #[test]
    fn test_parser_paragraph_plain_text() {
        let blocks = parse("Hello world", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "Hello world");
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_bold_text() {
        let blocks = parse("**bold**", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "bold");
                assert!(content[0].style.add_modifier.contains(Modifier::BOLD));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_italic_text() {
        let blocks = parse("*italic*", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "italic");
                assert!(content[0].style.add_modifier.contains(Modifier::ITALIC));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_strikethrough_text() {
        let blocks = parse("~~struck~~", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "struck");
                assert!(content[0]
                    .style
                    .add_modifier
                    .contains(Modifier::CROSSED_OUT));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_nested_bold_italic() {
        let blocks = parse("***bold italic***", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "bold italic");
                let mods = content[0].style.add_modifier;
                assert!(mods.contains(Modifier::BOLD));
                assert!(mods.contains(Modifier::ITALIC));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_inline_code() {
        let blocks = parse("Use `fmt` here", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 3);
                assert_eq!(content[0].text, "Use ");
                assert_eq!(content[1].text, "fmt");
                assert_eq!(content[1].style, default_code_style());
                assert_eq!(content[2].text, " here");
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_thematic_break() {
        let blocks = parse("---", h());
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], RenderedBlock::ThematicBreak));
    }

    #[test]
    fn test_parser_soft_break() {
        let blocks = parse("line one\nline two", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 3);
                assert_eq!(content[0].text, "line one");
                assert_eq!(content[1].text, " ");
                assert_eq!(content[2].text, "line two");
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_hard_break() {
        let blocks = parse("line one\\\nline two", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert!(content.iter().any(|s| s.text == "\n"));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_empty_input() {
        let blocks = parse("", h());
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_parser_heading_styles_are_distinct() {
        let s1 = default_heading_style(1);
        let s2 = default_heading_style(2);
        let s3 = default_heading_style(3);
        assert_ne!(s1.fg, s2.fg);
        assert_ne!(s2.fg, s3.fg);
    }

    #[test]
    fn test_parser_skips_unrecognized_blocks() {
        // Use a list (not code block) since code blocks are now handled.
        let md = "- item one\n- item two\n\nAfter list";
        let blocks = parse(md, h());
        assert!(blocks
            .iter()
            .any(|b| matches!(b, RenderedBlock::Paragraph { .. })));
    }

    #[test]
    fn test_parser_link_text_preserved() {
        let blocks = parse("See [the docs](https://example.com) for details", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                let all_text: String = content.iter().map(|s| s.text.as_str()).collect();
                assert!(
                    all_text.contains("the docs"),
                    "link text should be preserved, got: {all_text}"
                );
                assert!(
                    all_text.contains("See"),
                    "surrounding text preserved, got: {all_text}"
                );
                assert!(
                    all_text.contains("for details"),
                    "trailing text preserved, got: {all_text}"
                );
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_image_alt_text_preserved() {
        let blocks = parse("![alt text](image.png)", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                let all_text: String = content.iter().map(|s| s.text.as_str()).collect();
                assert!(
                    all_text.contains("alt text"),
                    "image alt text should be preserved, got: {all_text}"
                );
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_parser_bold_inside_link() {
        let blocks = parse("[**bold link**](url)", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content.len(), 1);
                assert_eq!(content[0].text, "bold link");
                assert!(content[0].style.add_modifier.contains(Modifier::BOLD));
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    // ── Phase 2: Code block tests ───────────────────────────────

    #[test]
    fn test_parser_fenced_code_block_with_language() {
        let md = "```rust\nfn main() {}\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock {
                language,
                highlighted_lines,
            } => {
                assert_eq!(language, "rust");
                assert!(!highlighted_lines.is_empty());
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_fenced_code_block_empty_language() {
        let md = "```\nsome code\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock { language, .. } => {
                assert!(language.is_empty());
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_indented_code_block() {
        let md = "    indented code\n    more code\n";
        let blocks = parse(md, h());
        assert!(
            blocks.iter().any(|b| matches!(b, RenderedBlock::CodeBlock { .. })),
            "indented code should produce CodeBlock"
        );
    }

    #[test]
    fn test_parser_inline_code_still_styled_span() {
        let blocks = parse("Use `code` inline", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert!(content.iter().any(|s| s.text == "code"));
            }
            _ => panic!("expected Paragraph, not CodeBlock"),
        }
    }

    #[test]
    fn test_parser_code_block_content_preserved() {
        let md = "```python\ndef hello():\n    print(\"world\")\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock {
                highlighted_lines, ..
            } => {
                let all_text: String = highlighted_lines
                    .iter()
                    .flat_map(|line| line.spans.iter())
                    .map(|span| span.content.as_ref())
                    .collect();
                assert!(all_text.contains("def"), "should contain 'def'");
                assert!(all_text.contains("hello"), "should contain 'hello'");
                assert!(all_text.contains("print"), "should contain 'print'");
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_code_block_followed_by_paragraph() {
        let md = "```rust\ncode\n```\n\nAfter code";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], RenderedBlock::CodeBlock { .. }));
        assert!(matches!(&blocks[1], RenderedBlock::Paragraph { .. }));
    }

    #[test]
    fn test_parser_empty_code_block() {
        let md = "```\n```";
        let blocks = parse(md, h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::CodeBlock {
                highlighted_lines, ..
            } => {
                assert!(
                    highlighted_lines.is_empty(),
                    "empty code block should produce no lines"
                );
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_parser_list_with_paragraphs_emits_no_stray_paragraphs() {
        // pulldown-cmark wraps list items in Tag::Paragraph when separated by blank lines.
        // The Skipping guard must suppress those inner paragraphs.
        let md = "- First item\n\n- Second item\n\nAfter list";
        let blocks = parse(md, h());
        let para_count = blocks
            .iter()
            .filter(|b| matches!(b, RenderedBlock::Paragraph { .. }))
            .count();
        assert_eq!(
            para_count, 1,
            "only the paragraph after the list should appear, got {para_count}"
        );
    }

    // ── Font slot strategy tests ────────────────────────────────

    #[test]
    fn test_parser_heading_h4_bold_italic() {
        let blocks = parse("#### Sub-heading", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Heading { level, content } => {
                assert_eq!(*level, 4);
                let mods = content[0].style.add_modifier;
                assert!(mods.contains(Modifier::BOLD), "h4 should have BOLD");
                assert!(mods.contains(Modifier::ITALIC), "h4 should have ITALIC");
            }
            _ => panic!("expected Heading block"),
        }
    }

    #[test]
    fn test_parser_heading_styles_distinct_modifiers() {
        let h1 = default_heading_style(1);
        let h4 = default_heading_style(4);
        // h1 has BOLD only
        assert!(h1.add_modifier.contains(Modifier::BOLD));
        assert!(!h1.add_modifier.contains(Modifier::ITALIC));
        // h4 has BOLD + ITALIC
        assert!(h4.add_modifier.contains(Modifier::BOLD));
        assert!(h4.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_parser_inline_code_has_bold_italic() {
        let style = default_code_style();
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "inline code should have BOLD"
        );
        assert!(
            style.add_modifier.contains(Modifier::ITALIC),
            "inline code should have ITALIC"
        );
    }

    #[test]
    fn test_parser_link_text_has_italic() {
        let blocks = parse("[click here](https://example.com)", h());
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            RenderedBlock::Paragraph { content } => {
                assert_eq!(content[0].text, "click here");
                assert!(
                    content[0].style.add_modifier.contains(Modifier::ITALIC),
                    "link text should have ITALIC"
                );
            }
            _ => panic!("expected Paragraph block"),
        }
    }

    #[test]
    fn test_font_slots_file_parses_without_panic() {
        let source = include_str!("../testdata/font-slots.md");
        let blocks = parse(source, h());
        assert!(blocks.len() > 20, "font-slots.md should produce many blocks");
        // Verify it contains all expected block types.
        let has_heading = blocks.iter().any(|b| matches!(b, RenderedBlock::Heading { .. }));
        let has_paragraph = blocks.iter().any(|b| matches!(b, RenderedBlock::Paragraph { .. }));
        let has_code = blocks.iter().any(|b| matches!(b, RenderedBlock::CodeBlock { .. }));
        let has_rule = blocks.iter().any(|b| matches!(b, RenderedBlock::ThematicBreak));
        assert!(has_heading, "should have headings");
        assert!(has_paragraph, "should have paragraphs");
        assert!(has_code, "should have code blocks");
        assert!(has_rule, "should have thematic breaks");
    }

    // ── Security regression tests ────────────────────────────────

    #[test]
    fn test_parser_info_string_first_word_only() {
        // pulldown-cmark yields the full info string — we must take only first word.
        // Formats like "rust,no_run", "python title=\"x.py\"" are common in docs.
        let cases = [
            ("```rust,no_run\ncode\n```", "rust"),
            ("```python title=\"x.py\"\ncode\n```", "python"),
            ("```javascript highlight=true\ncode\n```", "javascript"),
            ("```   rust   \ncode\n```", "rust"), // leading/trailing spaces trimmed by pulldown-cmark
        ];
        for (md, expected_lang) in cases {
            let blocks = parse(md, h());
            assert_eq!(blocks.len(), 1, "input: {md}");
            match &blocks[0] {
                RenderedBlock::CodeBlock { language, .. } => {
                    assert_eq!(
                        language, expected_lang,
                        "info string '{md}' should yield language '{expected_lang}', got '{language}'"
                    );
                }
                _ => panic!("expected CodeBlock for: {md}"),
            }
        }
    }
