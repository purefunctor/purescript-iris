use test_each_file::test_each_file;

test_each_file! { in "./compiler-frontend/parsing/tests/parser" => |content: &str| {
    let content = content.replace("\r\n", "\n").replace("\r", "\n");
    let lexed = lexing::lex(&content);
    let tokens = lexing::layout(&lexed);
    let (parsed, errors) = parsing::parse(&lexed, &tokens);
    let node = parsed.syntax_node();
    insta::with_settings!({ omit_expression => true }, {
        insta::assert_debug_snapshot!((node.debug(&content), errors));
    });
}}

test_each_file! { in "./compiler-frontend/parsing/tests/parser" as lossless => |content: &str| {
    let content = content.replace("\r\n", "\n").replace("\r", "\n");
    let lexed = lexing::lex(&content);
    let tokens = lexing::layout(&lexed);
    let (parsed, _) = parsing::parse(&lexed, &tokens);
    let node = parsed.syntax_node();
    assert_eq!(node.text(&content), content);
}}

test_each_file! { in "./compiler-frontend/parsing/tests/parser" as stability => |content: &str| {
    let content = content.replace("\r\n", "\n").replace("\r", "\n");
    let lexed = lexing::lex(&content);
    for index in 0..lexed.len() - 1 {
        let partial = lexed.text_in_range(0..index + 1);
        let lexed = lexing::lex(partial);
        let tokens = lexing::layout(&lexed);
        let (parsed, _) = parsing::parse(&lexed, &tokens);
        let node = parsed.syntax_node();
        assert_eq!(node.text(partial), partial);
    }
}}
