use syntax::{SyntaxKind, TokenSet};

use super::{NodeMarker, Parser, binders, binding, generic, names, types};

pub fn expression(p: &mut Parser) {
    let mut m = p.start();

    expression_1(p);
    if p.eat(SyntaxKind::DOUBLE_COLON) {
        types::type_(p);
        m.end(p, SyntaxKind::ExpressionTyped);
    } else {
        m.cancel(p);
    }
}

fn expression_1(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    expression_2(p);
    while p.at_in(names::OPERATOR) {
        let mut n = p.start();
        let mut o = p.start();
        names::operator(p);
        o.end(p, SyntaxKind::TermOperator);
        expression_2(p);
        n.end(p, SyntaxKind::ExpressionOperatorPair);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::ExpressionOperatorChain);
    } else {
        m.cancel(p);
    }
}

fn expression_2(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    expression_3(p);
    while p.at(SyntaxKind::TICK) {
        let mut m = p.start();
        tick_expression(p);
        expression_3(p);
        m.end(p, SyntaxKind::ExpressionInfixPair);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::ExpressionInfixChain);
    } else {
        m.cancel(p);
    }
}

fn tick_expression(p: &mut Parser) {
    let mut m = p.start();
    p.expect(SyntaxKind::TICK);
    tick_expression_1(p);
    p.expect(SyntaxKind::TICK);
    m.end(p, SyntaxKind::ExpressionTick);
}

fn tick_expression_1(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    expression_3(p);
    while p.at_in(names::OPERATOR) {
        names::operator(p);
        expression_3(p);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::ExpressionOperatorChain);
    } else {
        m.cancel(p);
    }
}

fn expression_3(p: &mut Parser) {
    let mut m = p.start();

    if p.eat(SyntaxKind::MINUS) {
        expression_3(p);
        m.end(p, SyntaxKind::ExpressionNegate);
    } else {
        expression_4(p);
        m.cancel(p);
    }
}

fn expression_4(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    expression_5(p);
    while at_argument(p) {
        expression_argument(p);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::ExpressionApplicationChain);
    } else {
        m.cancel(p);
    }
}

fn at_argument(p: &Parser) -> bool {
    p.at(SyntaxKind::AT) || p.at_in(ARGUMENT_START)
}

fn expression_argument(p: &mut Parser) {
    let mut m = p.start();
    if p.eat(SyntaxKind::AT) {
        types::type_atom(p);
        m.end(p, SyntaxKind::ExpressionTypeArgument);
    } else {
        expression_5(p);
        m.end(p, SyntaxKind::ExpressionTermArgument);
    }
}

fn expression_5(p: &mut Parser) {
    let mut m = p.start();

    if p.at(SyntaxKind::IF) {
        expression_if_then_else(p, m);
    } else if p.at(SyntaxKind::LET) {
        expression_let(p, m);
    } else if p.at(SyntaxKind::BACKSLASH) {
        expression_lambda(p, m);
    } else if p.at(SyntaxKind::CASE) {
        expression_case(p, m);
    } else if p.at(SyntaxKind::DO) {
        expression_do(p, m);
    } else if p.at(SyntaxKind::ADO) {
        expression_ado(p, m);
    } else if p.at_in(EXPRESSION_ATOM_START) {
        expression_6(p);
        m.cancel(p);
    } else {
        m.cancel(p);
    }
}

fn expression_if_then_else(p: &mut Parser, mut m: NodeMarker) {
    p.expect(SyntaxKind::IF);
    let mut n = p.start();
    expression(p);
    n.end(p, SyntaxKind::ExpressionIf);
    p.expect(SyntaxKind::THEN);
    let mut n = p.start();
    expression(p);
    n.end(p, SyntaxKind::ExpressionThen);
    p.expect(SyntaxKind::ELSE);
    let mut n = p.start();
    expression(p);
    n.end(p, SyntaxKind::ExpressionElse);
    m.end(p, SyntaxKind::ExpressionIfThenElse);
}

fn expression_let(p: &mut Parser, mut m: NodeMarker) {
    p.expect(SyntaxKind::LET);
    binding::let_binding_statements(p);
    p.expect(SyntaxKind::IN);
    expression(p);
    m.end(p, SyntaxKind::ExpressionLetIn);
}

const LAMBDA_BINDERS_END: TokenSet = TokenSet::new(&[SyntaxKind::RIGHT_ARROW]);

fn expression_lambda(p: &mut Parser, mut m: NodeMarker) {
    p.expect(SyntaxKind::BACKSLASH);
    generic::function_binders(p, LAMBDA_BINDERS_END);
    p.expect(SyntaxKind::RIGHT_ARROW);
    expression(p);
    m.end(p, SyntaxKind::ExpressionLambda);
}

fn expression_case(p: &mut Parser, mut m: NodeMarker) {
    p.expect(SyntaxKind::CASE);
    case_trunk(p);
    p.expect(SyntaxKind::OF);
    case_branches(p);
    m.end(p, SyntaxKind::ExpressionCaseOf);
}

fn case_trunk(p: &mut Parser) {
    let mut m = p.start();
    while !p.at(SyntaxKind::OF) && !p.at_eof() {
        if p.at_in(EXPRESSION_START) {
            expression(p);
            if p.at(SyntaxKind::COMMA) && p.at_next(SyntaxKind::OF) {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::OF) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(EXPRESSION_ATOM_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in case trunk");
        }
    }
    m.end(p, SyntaxKind::CaseTrunk);
}

fn case_branches(p: &mut Parser) {
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
    while !p.at(SyntaxKind::LAYOUT_END) && !p.at_eof() {
        case_branch(p);
        recover_until_end(p, "Unexpected tokens in case branch");
        if !p.at(SyntaxKind::LAYOUT_END) {
            p.expect(SyntaxKind::LAYOUT_SEPARATOR);
        }
    }
    p.expect(SyntaxKind::LAYOUT_END);
    m.end(p, SyntaxKind::CaseBranches);
}

fn case_branch(p: &mut Parser) {
    let mut m = p.start();
    case_branch_binders(p);
    generic::unconditional_or_conditionals(p, SyntaxKind::RIGHT_ARROW);
    m.end(p, SyntaxKind::CaseBranch);
}

const BINDERS_LIST_RECOVERY: TokenSet =
    TokenSet::new(&[SyntaxKind::LAYOUT_SEPARATOR, SyntaxKind::LAYOUT_END]);

fn case_branch_binders(p: &mut Parser) {
    let mut m = p.start();
    while !p.at(SyntaxKind::RIGHT_ARROW) && !p.at(SyntaxKind::PIPE) && !p.at_eof() {
        if p.at_in(binders::BINDER_START) {
            binders::binder_1(p);
            if p.at(SyntaxKind::COMMA)
                && (p.at_next(SyntaxKind::RIGHT_ARROW) || p.at_next(SyntaxKind::PIPE))
            {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::RIGHT_ARROW) && !p.at(SyntaxKind::PIPE) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(BINDERS_LIST_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in branch binders");
        }
    }
    m.end(p, SyntaxKind::CaseBranchBinders);
}

const DO_STATEMENT_START: TokenSet =
    TokenSet::new(&[SyntaxKind::LET]).union(EXPRESSION_START).union(binders::BINDER_START);

fn expression_do(p: &mut Parser, mut m: NodeMarker) {
    p.expect(SyntaxKind::DO);
    do_statements(p);
    m.end(p, SyntaxKind::ExpressionDo);
}

fn do_statements(p: &mut Parser) {
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
    while p.at_in(DO_STATEMENT_START) {
        do_statement(p);
        recover_until_end(p, "Unexpected tokens in do statement");
        if !p.at(SyntaxKind::LAYOUT_END) {
            p.expect(SyntaxKind::LAYOUT_SEPARATOR);
        }
    }
    p.expect(SyntaxKind::LAYOUT_END);
    m.end(p, SyntaxKind::DoStatements);
}

fn do_statement(p: &mut Parser) {
    if p.at(SyntaxKind::LET) {
        do_statement_let(p);
    } else {
        p.alternative(binders::binder_left_arrow, do_statement_bind, do_statement_discard);
    }
}

fn do_statement_let(p: &mut Parser) {
    let mut m = p.start();
    p.expect(SyntaxKind::LET);
    binding::let_binding_statements(p);
    expression(p);
    m.end(p, SyntaxKind::DoStatementLet);
}

fn do_statement_bind(p: &mut Parser) {
    let mut m = p.start();
    binders::binder_left_arrow(p);
    expression(p);
    m.end(p, SyntaxKind::DoStatementBind);
}

fn do_statement_discard(p: &mut Parser) {
    let mut m = p.start();
    expression(p);
    m.end(p, SyntaxKind::DoStatementDiscard);
}

fn expression_ado(p: &mut Parser, mut m: NodeMarker) {
    p.expect(SyntaxKind::ADO);
    do_statements(p);
    p.expect(SyntaxKind::IN);
    expression(p);
    m.end(p, SyntaxKind::ExpressionAdo);
}

fn expression_6(p: &mut Parser) {
    let mut m = p.start();
    expression_7(p);
    if is_record_update(p) {
        record_updates(p);
        m.end(p, SyntaxKind::ExpressionRecordUpdate);
    } else {
        m.cancel(p);
    }
}

fn is_record_update(p: &Parser) -> bool {
    p.nth_at(0, SyntaxKind::LEFT_CURLY)
        && p.nth_at_in(1, names::RECORD_LABEL)
        && (p.nth_at(2, SyntaxKind::EQUAL) || p.nth_at(2, SyntaxKind::LEFT_CURLY))
}

fn record_updates(p: &mut Parser) {
    let mut m = p.start();
    p.expect(SyntaxKind::LEFT_CURLY);
    while !p.at(SyntaxKind::RIGHT_CURLY) && !p.at_eof() {
        if p.at_in(names::RECORD_LABEL) {
            record_update(p);
            if p.at(SyntaxKind::COMMA) && p.at_next(SyntaxKind::RIGHT_CURLY) {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::RIGHT_CURLY) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(EXPRESSION_ATOM_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in record update");
        }
    }
    p.expect(SyntaxKind::RIGHT_CURLY);
    m.end(p, SyntaxKind::RecordUpdates);
}

fn record_update(p: &mut Parser) {
    let mut m = p.start();
    names::label(p);
    if p.eat(SyntaxKind::EQUAL) {
        expression(p);
        m.end(p, SyntaxKind::RecordUpdateLeaf);
    } else if p.at(SyntaxKind::LEFT_CURLY) {
        record_updates(p);
        m.end(p, SyntaxKind::RecordUpdateBranch);
    } else {
        m.cancel(p);
    }
}

fn expression_7(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    expression_atom(p);
    while p.at(SyntaxKind::PERIOD) {
        let mut label = p.start();
        p.expect(SyntaxKind::PERIOD);
        names::label(p);
        label.end(p, SyntaxKind::RecordAccessLabel);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::ExpressionRecordAccess);
    } else {
        m.cancel(p);
    }
}

fn expression_atom(p: &mut Parser) {
    let mut m = p.start();

    if p.at_in(names::LOWER) {
        names::lower(p);
        m.end(p, SyntaxKind::ExpressionVariable);
    } else if p.at(SyntaxKind::UPPER) {
        names::upper(p);
        m.end(p, SyntaxKind::ExpressionConstructor);
    } else if p.at_in(names::OPERATOR_NAME) {
        names::operator_name(p);
        m.end(p, SyntaxKind::ExpressionOperatorName);
    } else if p.eat(SyntaxKind::UNDERSCORE) {
        m.end(p, SyntaxKind::ExpressionSection);
    } else if p.eat(SyntaxKind::HOLE) {
        m.end(p, SyntaxKind::ExpressionHole);
    } else if p.eat(SyntaxKind::STRING) || p.eat(SyntaxKind::RAW_STRING) {
        m.end(p, SyntaxKind::ExpressionString);
    } else if p.eat(SyntaxKind::CHAR) {
        m.end(p, SyntaxKind::ExpressionChar);
    } else if p.eat(SyntaxKind::TRUE) {
        m.end(p, SyntaxKind::ExpressionTrue);
    } else if p.eat(SyntaxKind::FALSE) {
        m.end(p, SyntaxKind::ExpressionFalse);
    } else if p.eat(SyntaxKind::INTEGER) {
        m.end(p, SyntaxKind::ExpressionInteger);
    } else if p.eat(SyntaxKind::NUMBER) {
        m.end(p, SyntaxKind::ExpressionNumber);
    } else if p.eat(SyntaxKind::LEFT_SQUARE) {
        expression_array(p, m);
    } else if p.eat(SyntaxKind::LEFT_CURLY) {
        expression_record(p, m);
    } else if p.eat(SyntaxKind::LEFT_PARENTHESIS) {
        expression(p);
        p.expect(SyntaxKind::RIGHT_PARENTHESIS);
        m.end(p, SyntaxKind::ExpressionParenthesized);
    } else {
        m.cancel(p);
    }
}

fn expression_array(p: &mut Parser, mut m: NodeMarker) {
    while !p.at(SyntaxKind::RIGHT_SQUARE) && !p.at_eof() {
        if p.at_in(EXPRESSION_START) {
            expression(p);
            if p.at(SyntaxKind::COMMA) && p.at_next(SyntaxKind::RIGHT_SQUARE) {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::RIGHT_SQUARE) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(EXPRESSION_ATOM_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in array");
        }
    }
    p.expect(SyntaxKind::RIGHT_SQUARE);
    m.end(p, SyntaxKind::ExpressionArray);
}

fn expression_record(p: &mut Parser, mut m: NodeMarker) {
    while !p.at(SyntaxKind::RIGHT_CURLY) && !p.at_eof() {
        if p.at_in(names::RECORD_LABEL) {
            generic::record_item(p, expression);
            if p.at(SyntaxKind::COMMA) && p.at_next(SyntaxKind::RIGHT_CURLY) {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::RIGHT_CURLY) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(EXPRESSION_ATOM_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in record");
        }
    }
    p.expect(SyntaxKind::RIGHT_CURLY);
    m.end(p, SyntaxKind::ExpressionRecord);
}

const EXPRESSION_ATOM_START: TokenSet = TokenSet::new(&[
    SyntaxKind::UPPER,
    SyntaxKind::UNDERSCORE,
    SyntaxKind::HOLE,
    SyntaxKind::STRING,
    SyntaxKind::RAW_STRING,
    SyntaxKind::CHAR,
    SyntaxKind::TRUE,
    SyntaxKind::FALSE,
    SyntaxKind::INTEGER,
    SyntaxKind::NUMBER,
    SyntaxKind::LEFT_SQUARE,
    SyntaxKind::LEFT_CURLY,
    SyntaxKind::LEFT_PARENTHESIS,
])
.union(names::LOWER)
.union(names::OPERATOR_NAME);

const ARGUMENT_START: TokenSet = TokenSet::new(&[
    SyntaxKind::IF,
    SyntaxKind::LET,
    SyntaxKind::BACKSLASH,
    SyntaxKind::CASE,
    SyntaxKind::DO,
    SyntaxKind::ADO,
])
.union(EXPRESSION_ATOM_START);

pub(super) const EXPRESSION_START: TokenSet =
    ARGUMENT_START.union(TokenSet::new(&[SyntaxKind::MINUS]));

const EXPRESSION_ATOM_RECOVERY: TokenSet =
    TokenSet::new(&[SyntaxKind::LAYOUT_SEPARATOR, SyntaxKind::LAYOUT_END]);
