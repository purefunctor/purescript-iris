use syntax::{SyntaxKind, TokenSet};

use super::{Parser, binders, expressions, generic};

const LET_BINDING_START: TokenSet = TokenSet::new(&[SyntaxKind::LOWER])
    .union(expressions::EXPRESSION_START)
    .union(binders::BINDER_START);

pub(super) fn let_binding_statements(p: &mut Parser) {
    let mut m = p.start();
    let recover_until_end = |p: &mut Parser, m: &'static str| {
        let mut e = None;
        while !p.at(SyntaxKind::LAYOUT_SEPARATOR) && !p.at(SyntaxKind::LAYOUT_END) && !p.at_eof() {
            if e.is_none() {
                e = Some(p.start());
                p.error(m);
            }
            p.consume();
        }
        if let Some(mut e) = e {
            e.end(p, SyntaxKind::ERROR);
        }
    };
    p.expect(SyntaxKind::LAYOUT_START);
    while p.at_in(LET_BINDING_START) {
        p.alternative(
            let_binding_signature_or_equation_head,
            let_binding_signature_or_equation,
            let_binding_pattern,
        );
        recover_until_end(p, "Unexpected tokens in let binding statement");
        if !p.at(SyntaxKind::LAYOUT_END) {
            p.expect(SyntaxKind::LAYOUT_SEPARATOR);
        }
    }
    p.expect(SyntaxKind::LAYOUT_END);
    m.end(p, SyntaxKind::LetBindingStatements);
}

fn let_binding_signature_or_equation_head(p: &mut Parser) {
    generic::signature_or_equation_head(p);
}

fn let_binding_signature_or_equation(p: &mut Parser) {
    generic::signature_or_equation(
        p,
        SyntaxKind::LetBindingSignature,
        SyntaxKind::LetBindingEquation,
    );
}

fn let_binding_pattern(p: &mut Parser) {
    let mut m = p.start();
    binders::binder_1(p);
    p.expect(SyntaxKind::EQUAL);
    generic::where_expression(p);
    m.end(p, SyntaxKind::LetBindingPattern);
}
