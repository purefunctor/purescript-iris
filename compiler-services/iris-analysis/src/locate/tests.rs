use syntax::TextSize;

use crate::position::{PositionConverter, PositionEncoding, Utf8Position};

fn utf8_position_to_offset(content: &str, position: Utf8Position) -> Option<TextSize> {
    PositionConverter::new(content, PositionEncoding::Utf8).utf8_position_to_offset(position)
}

#[test]
fn zero_on_blank_line() {
    let content = "";
    let position = Utf8Position { line: 0, column: 0 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(0)));
}

#[test]
fn zero_or_lf_line() {
    let content = "\n";
    let position = Utf8Position { line: 0, column: 0 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(0)));
}

#[test]
fn zero_or_crlf_line() {
    let content = "\r\n";
    let position = Utf8Position { line: 0, column: 0 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(0)));
}

#[test]
fn last_on_line() {
    let content = "abcdef";
    let position = Utf8Position { line: 0, column: 6 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(6)));
}

#[test]
fn last_on_line_clamp() {
    let content = "abcdef";
    let position = Utf8Position { line: 0, column: 600 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(6)));
}

#[test]
fn last_on_lf_line() {
    let content = "abcdef\n";
    let position = Utf8Position { line: 0, column: 6 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(6)));
}

#[test]
fn last_on_crlf_line_clamp() {
    let content = "abcdef\r\n";
    let position = Utf8Position { line: 0, column: 600 };

    let offset = utf8_position_to_offset(content, position);
    assert_eq!(offset, Some(TextSize::new(6)));
}

mod text_range_to_range {
    use syntax::{TextRange, TextSize};

    use crate::position::{PositionConverter, PositionEncoding};

    /// Extracts a slice from `content` using `_` anchors to mark
    /// the start and end. The content is returned with the anchors
    /// removed.
    ///
    /// Example: `"hello _world_"` -> ("hello world", TextRange(6, 11))
    fn extract_range(content: &str) -> (String, TextRange) {
        let mut clean = String::new();
        let mut positions = Vec::new();
        let mut byte_offset = 0u32;

        for ch in content.chars() {
            if ch == '_' {
                positions.push(TextSize::new(byte_offset));
            } else {
                clean.push(ch);
                byte_offset += ch.len_utf8() as u32;
            }
        }

        assert_eq!(positions.len(), 2, "Expected exactly 2 '_' anchors for range");
        (clean, TextRange::new(positions[0], positions[1]))
    }

    fn format_test_case(input: &str) -> String {
        let (content, range) = extract_range(input);
        let positions = PositionConverter::new(&content, PositionEncoding::Utf8);

        let utf8_range = positions.text_range_to_utf8_range(range).unwrap();
        let text_range = {
            let start = positions.utf8_position_to_offset(utf8_range.start).unwrap();
            let end = positions.utf8_position_to_offset(utf8_range.end).unwrap();
            TextRange::new(start, end)
        };

        format!(
            "Content: {}\nRange: {}:{} -> {}:{}\nRoundtrip: {}",
            &content[range],
            utf8_range.start.line,
            utf8_range.start.column,
            utf8_range.end.line,
            utf8_range.end.column,
            range == text_range,
        )
    }

    #[test]
    fn simple() {
        insta::assert_snapshot!(format_test_case("_hello, world_"), @r"
        Content: hello, world
        Range: 0:0 -> 0:12
        Roundtrip: true
        ");
    }

    #[test]
    fn partial() {
        insta::assert_snapshot!(format_test_case("hello, _world_"), @r"
        Content: world
        Range: 0:7 -> 0:12
        Roundtrip: true
        ");
    }

    #[test]
    fn unicode_full() {
        insta::assert_snapshot!(format_test_case("_content ∷ Type_"), @r"
        Content: content ∷ Type
        Range: 0:0 -> 0:16
        Roundtrip: true
        ");
    }

    #[test]
    fn unicode_partial() {
        insta::assert_snapshot!(format_test_case("content _∷ Type_"), @r"
        Content: ∷ Type
        Range: 0:8 -> 0:16
        Roundtrip: true
        ");
    }

    #[test]
    fn unicode_ending() {
        insta::assert_snapshot!(format_test_case("_content ∷_ Type"), @r"
        Content: content ∷
        Range: 0:0 -> 0:11
        Roundtrip: true
        ");
    }

    #[test]
    fn empty_range() {
        insta::assert_snapshot!(format_test_case("hello__ world"), @r"
        Content: 
        Range: 0:5 -> 0:5
        Roundtrip: true
        ");
    }

    #[test]
    fn emoji_single() {
        insta::assert_snapshot!(format_test_case("hello _😀_ world"), @r"
        Content: 😀
        Range: 0:6 -> 0:10
        Roundtrip: true
        ");
    }

    #[test]
    fn emoji_multiple() {
        insta::assert_snapshot!(format_test_case("_🎉🎊🎈_"), @r"
        Content: 🎉🎊🎈
        Range: 0:0 -> 0:12
        Roundtrip: true
        ");
    }

    #[test]
    fn multiline_within_line() {
        insta::assert_snapshot!(format_test_case("line1\nl_ine2_\nline3"), @r"
        Content: ine2
        Range: 1:1 -> 1:5
        Roundtrip: true
        ");
    }

    #[test]
    fn multiline_spanning() {
        insta::assert_snapshot!(format_test_case("li_ne1\nline2_\nline3"), @r"
        Content: ne1
        line2
        Range: 0:2 -> 1:5
        Roundtrip: true
        ");
    }

    #[test]
    fn multiline_crlf() {
        insta::assert_snapshot!(format_test_case("line1\r\n_line2_\r\nline3"), @r"
        Content: line2
        Range: 1:0 -> 1:5
        Roundtrip: true
        ");
    }

    #[test]
    fn multiline_spanning_crlf() {
        insta::assert_snapshot!(format_test_case("fir_st\r\nsec_ond\r\nthird"), @r"
        Content: st
        sec
        Range: 0:3 -> 1:3
        Roundtrip: true
        ");
    }

    #[test]
    fn multiline_with_empty_lines() {
        insta::assert_snapshot!(format_test_case("_first\n\nthird_"), @r"
        Content: first

        third
        Range: 0:0 -> 2:5
        Roundtrip: true
        ");
    }

    #[test]
    fn multiline_unicode() {
        insta::assert_snapshot!(format_test_case("type _∷ Type\nvalue ∷_ Int\ndata ∷ Data"), @r"
        Content: ∷ Type
        value ∷
        Range: 0:5 -> 1:9
        Roundtrip: true
        ");
    }

    #[test]
    fn file_start() {
        insta::assert_snapshot!(format_test_case("_hello_\nworld"), @r"
        Content: hello
        Range: 0:0 -> 0:5
        Roundtrip: true
        ");
    }

    #[test]
    fn file_end() {
        insta::assert_snapshot!(format_test_case("hello\n_world_"), @r"
        Content: world
        Range: 1:0 -> 1:5
        Roundtrip: true
        ");
    }

    #[test]
    fn entire_file() {
        insta::assert_snapshot!(format_test_case("_first\nsecond\nthird_"), @r"
        Content: first
        second
        third
        Range: 0:0 -> 2:5
        Roundtrip: true
        ");
    }

    #[test]
    fn empty_file() {
        insta::assert_snapshot!(format_test_case("__"), @r"
        Content: 
        Range: 0:0 -> 0:0
        Roundtrip: true
        ");
    }

    #[test]
    fn mixed_line_endings() {
        insta::assert_snapshot!(format_test_case("first\n_second\r\nthird_\nfourth"), @r"
        Content: second
        third
        Range: 1:0 -> 2:5
        Roundtrip: true
        ");
    }

    #[test]
    fn line_boundary() {
        insta::assert_snapshot!(format_test_case("lin_e1_\nline2"), @r"
        Content: e1
        Range: 0:3 -> 0:5
        Roundtrip: true
        ");
    }

    #[test]
    fn starting_at_newline() {
        insta::assert_snapshot!(format_test_case("line1_\nline2_"), @r"
        Content: 
        line2
        Range: 0:5 -> 1:5
        Roundtrip: true
        ");
    }
}
