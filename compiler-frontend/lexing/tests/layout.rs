use lexing::Lexed;
use syntax::SyntaxKind;
use test_each_file::test_each_file;

fn print_tokens(lexed: &Lexed, tokens: &[SyntaxKind]) -> String {
    let mut buffer = String::new();
    let mut index = 0;

    for token in tokens {
        match token {
            SyntaxKind::LAYOUT_START => {
                buffer.push('{');
            }
            SyntaxKind::LAYOUT_SEPARATOR => {
                buffer.push(';');
            }
            SyntaxKind::LAYOUT_END => {
                buffer.push('}');
            }
            i => {
                if let Some(annotation) = lexed.annotation(index) {
                    buffer.push_str(annotation);
                }
                if let Some(qualifier) = lexed.qualifier(index) {
                    buffer.push_str(qualifier);
                }
                if matches!(i, SyntaxKind::END_OF_FILE) {
                    buffer.push_str("<eof>");
                } else {
                    buffer.push_str(lexed.text(index));
                }
                index += 1;
            }
        }
    }
    buffer
}

test_each_file! { in "./compiler-frontend/lexing/tests/layout" => |content: &str| {
    let lexed = lexing::lex(content);
    let tokens = lexing::layout(&lexed);
    insta::with_settings!({ omit_expression => true }, {
        insta::assert_snapshot!(print_tokens(&lexed, &tokens));
    });
}}
