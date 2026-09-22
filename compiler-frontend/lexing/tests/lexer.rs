use test_each_file::test_each_file;

test_each_file! { in "./compiler-frontend/lexing/tests/lexer" => |content: &str| {
    use std::fmt::Write;

    let content = content.replace("\r\n", "\n").replace("\r", "\n");
    let lexed = lexing::lex(&content);

    let mut snapshot = String::new();
    for index in 0..lexed.len() {
        writeln!(snapshot, "annotation : {:?}", lexed.annotation(index).unwrap_or_default()).unwrap();
        writeln!(snapshot, " qualifier : {:?}", lexed.qualifier(index).unwrap_or_default()).unwrap();
        writeln!(snapshot, "      kind : {:?}", lexed.kind(index)).unwrap();
        writeln!(snapshot, "      text : {:?}", lexed.text(index)).unwrap();
        writeln!(snapshot, "  position : {:?}", lexed.position(index)).unwrap();
        writeln!(snapshot, "     error : {:?}", lexed.error(index)).unwrap();
        writeln!(snapshot).unwrap();
    }

    insta::with_settings!({ description => content, omit_expression => true }, {
        insta::assert_snapshot!(snapshot);
    })
}}

test_each_file! { in "./compiler-frontend/lexing/tests/lexer" as reconstruction => |content: &str| {
    // This test asserts that the original content can be reconstructed from the lexed tokens.
    let lexed = lexing::lex(content);

    let mut result = String::new();
    for index in 0..lexed.len() {
        if let Some(annotation) = lexed.annotation(index) {
            result.push_str(annotation);
        }
        if let Some(qualifier) = lexed.qualifier(index) {
            result.push_str(qualifier);
        }
        result.push_str(lexed.text(index));
    }

    assert_eq!(content, result);
}}

mod invariants {
    #[test]
    fn zero_width_annotation_is_previous_token() {
        let lexed = lexing::lex("1+2");

        let one = lexed.info(0);
        let two = lexed.info(1);

        assert_eq!(one.token, two.annotation);
        assert_eq!(one.token, two.qualifier);

        let lexed = lexing::lex("(..");

        let open = lexed.info(0);
        let dot = lexed.info(1);

        assert_eq!(open.token, dot.annotation);
        assert_eq!(open.token, dot.qualifier);
    }

    #[test]
    fn trailing_qualifier() {
        let lexed = lexing::lex("Hello.");
        assert!(lexed.error(0).is_some());
    }
}
