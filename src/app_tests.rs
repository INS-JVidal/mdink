    use super::*;
    use crate::layout::{DocumentLine, PreRenderedDocument};

    fn make_doc(line_count: usize) -> PreRenderedDocument {
        let lines = (0..line_count).map(|_| DocumentLine::Empty).collect();
        PreRenderedDocument {
            lines,
            total_height: line_count,
        }
    }

    fn make_app(doc_lines: usize, viewport: usize) -> App {
        let mut app = App::new(make_doc(doc_lines), "test.md".to_string());
        app.viewport_height = viewport;
        app
    }

    #[test]
    fn test_app_scroll_down_clamped() {
        let mut app = make_app(10, 5);
        app.scroll_down(100);
        // max_scroll = 10 - 5 = 5
        assert_eq!(app.scroll_offset, 5);
    }

    #[test]
    fn test_app_scroll_up_floor_at_zero() {
        let mut app = make_app(10, 5);
        app.scroll_offset = 2;
        app.scroll_up(100);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_app_max_scroll_short_document() {
        let app = make_app(3, 10);
        // Document shorter than viewport → max_scroll is 0.
        assert_eq!(app.max_scroll(), 0);
    }

    #[test]
    fn test_app_max_scroll_exact_fit() {
        let app = make_app(10, 10);
        assert_eq!(app.max_scroll(), 0);
    }

    #[test]
    fn test_app_visible_range_at_top() {
        let app = make_app(20, 5);
        assert_eq!(app.visible_range(), 0..5);
    }

    #[test]
    fn test_app_visible_range_at_bottom() {
        let mut app = make_app(20, 5);
        app.scroll_to_bottom();
        assert_eq!(app.visible_range(), 15..20);
    }

    #[test]
    fn test_app_visible_range_short_document() {
        let app = make_app(3, 10);
        // Document is only 3 lines but viewport is 10.
        assert_eq!(app.visible_range(), 0..3);
    }

    #[test]
    fn test_app_scroll_percent_at_top() {
        let app = make_app(20, 5);
        assert_eq!(app.scroll_percent(), 0);
    }

    #[test]
    fn test_app_scroll_percent_at_bottom() {
        let mut app = make_app(20, 5);
        app.scroll_to_bottom();
        assert_eq!(app.scroll_percent(), 100);
    }

    #[test]
    fn test_app_scroll_percent_short_document() {
        let app = make_app(3, 10);
        // Document fits in viewport → 100%.
        assert_eq!(app.scroll_percent(), 100);
    }

    #[test]
    fn test_app_scroll_percent_zero_lines() {
        let app = make_app(0, 10);
        assert_eq!(app.scroll_percent(), 100);
    }

    #[test]
    fn test_app_scroll_percent_one_line() {
        let app = make_app(1, 10);
        assert_eq!(app.scroll_percent(), 100);
    }

    #[test]
    fn test_app_scroll_to_top() {
        let mut app = make_app(20, 5);
        app.scroll_offset = 10;
        app.scroll_to_top();
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_app_scroll_to_bottom() {
        let mut app = make_app(20, 5);
        app.scroll_to_bottom();
        assert_eq!(app.scroll_offset, 15);
    }

    #[test]
    fn test_app_handle_key_quit_q() {
        let mut app = make_app(10, 5);
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        app.handle_key(key);
        assert!(app.quit);
    }

    #[test]
    fn test_app_handle_key_quit_esc() {
        let mut app = make_app(10, 5);
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        app.handle_key(key);
        assert!(app.quit);
    }

    #[test]
    fn test_app_handle_key_scroll_j() {
        let mut app = make_app(20, 5);
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty());
        app.handle_key(key);
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn test_app_handle_key_scroll_k() {
        let mut app = make_app(20, 5);
        app.scroll_offset = 5;
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::empty());
        app.handle_key(key);
        assert_eq!(app.scroll_offset, 4);
    }
