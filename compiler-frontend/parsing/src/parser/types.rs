use syntax::{SyntaxKind, TokenSet};

use super::{Parser, names};

pub(super) fn type_(p: &mut Parser) {
    let mut m = p.start();

    type_1(p);
    if p.eat(SyntaxKind::DOUBLE_COLON) {
        type_(p);
        m.end(p, SyntaxKind::TypeKinded);
    } else {
        m.cancel(p);
    }
}

fn type_1(p: &mut Parser) {
    let mut m = p.start();

    if p.eat(SyntaxKind::FORALL) {
        type_variable_bindings(p);
        p.expect(SyntaxKind::PERIOD);
        type_1(p);
        m.end(p, SyntaxKind::TypeForall);
    } else {
        type_2(p);
        m.cancel(p);
    }
}

fn type_2(p: &mut Parser) {
    let mut m = p.start();

    type_3(p);
    if p.eat(SyntaxKind::RIGHT_ARROW) {
        type_1(p);
        m.end(p, SyntaxKind::TypeArrow);
    } else if p.eat(SyntaxKind::RIGHT_THICK_ARROW) {
        type_1(p);
        m.end(p, SyntaxKind::TypeConstrained);
    } else {
        m.cancel(p);
    }
}

pub(super) fn type_3(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    type_4(p);
    while p.at_in(names::OPERATOR) {
        let mut n = p.start();
        let mut o = p.start();
        names::operator(p);
        o.end(p, SyntaxKind::TypeOperator);
        type_4(p);
        n.end(p, SyntaxKind::TypeOperatorPair);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::TypeOperatorChain);
    } else {
        m.cancel(p);
    }
}

fn type_4(p: &mut Parser) {
    let mut m = p.start();

    if p.eat(SyntaxKind::MINUS) {
        p.expect(SyntaxKind::INTEGER);
        m.end(p, SyntaxKind::TypeInteger);
    } else {
        type_5(p);
        m.cancel(p);
    }
}

pub(super) fn type_5(p: &mut Parser) {
    let mut m = p.start();
    let mut i = 0;

    type_atom(p);
    while p.at_in(TYPE_ATOM_START) {
        type_atom(p);
        i += 1;
    }

    if i > 0 {
        m.end(p, SyntaxKind::TypeApplicationChain);
    } else {
        m.cancel(p);
    }
}

pub(super) fn type_atom(p: &mut Parser) {
    if p.at_in(names::LOWER) {
        type_variable(p);
        return;
    }

    let mut m = p.start();

    if p.at(SyntaxKind::UPPER) {
        names::upper(p);
        m.end(p, SyntaxKind::TypeConstructor);
    } else if p.at_in(names::OPERATOR_NAME) {
        names::operator_name(p);
        m.end(p, SyntaxKind::TypeOperatorName);
    } else if p.eat(SyntaxKind::STRING) || p.eat(SyntaxKind::RAW_STRING) {
        m.end(p, SyntaxKind::TypeString);
    } else if p.at(SyntaxKind::MINUS) || p.at(SyntaxKind::INTEGER) {
        p.eat(SyntaxKind::MINUS);
        p.eat(SyntaxKind::INTEGER);
        m.end(p, SyntaxKind::TypeInteger);
    } else if p.at(SyntaxKind::LEFT_PARENTHESIS) {
        type_parenthesis(p);
        m.cancel(p);
    } else if p.at(SyntaxKind::LEFT_CURLY) {
        type_record(p);
        m.cancel(p);
    } else if p.at(SyntaxKind::LEFT_SQUARE) {
        type_effect_set(p);
        m.cancel(p);
    } else if p.eat(SyntaxKind::HOLE) {
        m.end(p, SyntaxKind::TypeHole);
    } else if p.eat(SyntaxKind::UNDERSCORE) {
        m.end(p, SyntaxKind::TypeWildcard);
    } else {
        m.cancel(p);
    }
}

pub(super) fn type_variable(p: &mut Parser) {
    let mut m = p.start();
    p.eat_in(names::LOWER, SyntaxKind::LOWER);
    m.end(p, SyntaxKind::TypeVariable);
}

pub(super) const TYPE_ATOM_START: TokenSet = TokenSet::new(&[
    SyntaxKind::UPPER,
    SyntaxKind::STRING,
    SyntaxKind::RAW_STRING,
    SyntaxKind::MINUS,
    SyntaxKind::INTEGER,
    SyntaxKind::OPERATOR_NAME,
    SyntaxKind::LEFT_PARENTHESIS,
    SyntaxKind::LEFT_CURLY,
    SyntaxKind::LEFT_SQUARE,
    SyntaxKind::UNDERSCORE,
])
.union(names::LOWER);

const TYPE_VARIABLE_BINDING_START: TokenSet =
    TokenSet::new(&[SyntaxKind::AT, SyntaxKind::LEFT_PARENTHESIS]).union(names::LOWER);

const TYPE_VARIABLE_BINDING_RECOVERY: TokenSet =
    TokenSet::new(&[SyntaxKind::LAYOUT_SEPARATOR, SyntaxKind::LAYOUT_END]);

fn type_variable_bindings(p: &mut Parser) {
    while !p.at(SyntaxKind::PERIOD) && !p.at_eof() {
        if p.at_in(TYPE_VARIABLE_BINDING_START) {
            type_variable_binding(p);
        } else {
            if p.at_in(TYPE_VARIABLE_BINDING_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in variable bindings.");
        }
    }
}

fn type_variable_binding(p: &mut Parser) {
    let mut m = p.start();

    let closing = p.eat(SyntaxKind::LEFT_PARENTHESIS);

    p.eat(SyntaxKind::AT);
    p.expect_in(names::LOWER, SyntaxKind::LOWER, "Expected LOWER");

    if p.eat(SyntaxKind::DOUBLE_COLON) {
        type_(p);
    }

    if closing {
        p.expect(SyntaxKind::RIGHT_PARENTHESIS);
    }

    m.end(p, SyntaxKind::TypeVariableBinding);
}

fn type_parenthesis(p: &mut Parser) {
    if is_type_row(p) {
        type_row(p);
    } else if is_kinded_type_variable(p) {
        type_kinded_variable(p);
    } else {
        type_parenthesized(p);
    }
}

fn is_type_row(p: &Parser) -> bool {
    p.nth_at(1, SyntaxKind::RIGHT_PARENTHESIS)
        || p.nth_at(1, SyntaxKind::PIPE)
        || (p.nth_at_in(1, names::RECORD_LABEL) && p.nth_at(2, SyntaxKind::DOUBLE_COLON))
}

fn is_kinded_type_variable(p: &Parser) -> bool {
    p.nth_at(1, SyntaxKind::LEFT_PARENTHESIS)
        && p.nth_at_in(2, names::LOWER)
        && p.nth_at(3, SyntaxKind::RIGHT_PARENTHESIS)
        && p.nth_at(4, SyntaxKind::DOUBLE_COLON)
}

fn type_kinded_variable(p: &mut Parser) {
    let mut m = p.start();
    p.expect(SyntaxKind::LEFT_PARENTHESIS);
    let mut n = p.start();
    p.expect(SyntaxKind::LEFT_PARENTHESIS);
    let mut o = p.start();
    p.expect_in(names::LOWER, SyntaxKind::LOWER, "Expected LOWER");
    o.end(p, SyntaxKind::TypeVariable);
    p.expect(SyntaxKind::RIGHT_PARENTHESIS);
    n.end(p, SyntaxKind::TypeParenthesized);
    p.expect(SyntaxKind::DOUBLE_COLON);
    type_(p);
    p.expect(SyntaxKind::RIGHT_PARENTHESIS);
    m.end(p, SyntaxKind::TypeKinded);
}

fn type_parenthesized(p: &mut Parser) {
    let mut m = p.start();
    p.expect(SyntaxKind::LEFT_PARENTHESIS);
    type_(p);
    p.expect(SyntaxKind::RIGHT_PARENTHESIS);
    m.end(p, SyntaxKind::TypeParenthesized);
}

fn type_row(p: &mut Parser) {
    let mut m = p.start();

    p.expect(SyntaxKind::LEFT_PARENTHESIS);
    while !p.at(SyntaxKind::PIPE) && !p.at(SyntaxKind::RIGHT_PARENTHESIS) && !p.at_eof() {
        if p.at_in(names::RECORD_LABEL) {
            row_item(p);
            let ending = p.at_next(SyntaxKind::PIPE) || p.at_next(SyntaxKind::RIGHT_PARENTHESIS);
            if p.at(SyntaxKind::COMMA) && ending {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::PIPE) && !p.at(SyntaxKind::RIGHT_PARENTHESIS) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(TYPE_ROW_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in row");
        }
    }

    if p.at(SyntaxKind::PIPE) {
        row_tail(p);
    }

    p.expect(SyntaxKind::RIGHT_PARENTHESIS);
    m.end(p, SyntaxKind::TypeRow);
}

const TYPE_ROW_RECOVERY: TokenSet = TokenSet::new(&[
    SyntaxKind::PIPE,
    SyntaxKind::RIGHT_PARENTHESIS,
    SyntaxKind::LAYOUT_SEPARATOR,
    SyntaxKind::LAYOUT_END,
]);

fn row_item(p: &mut Parser) {
    let mut m = p.start();

    names::label(p);
    p.expect(SyntaxKind::DOUBLE_COLON);
    type_(p);

    m.end(p, SyntaxKind::TypeRowItem);
}

fn row_tail(p: &mut Parser) {
    let mut m = p.start();

    p.expect(SyntaxKind::PIPE);
    type_(p);

    m.end(p, SyntaxKind::TypeRowTail);
}

fn type_record(p: &mut Parser) {
    let mut m = p.start();

    p.expect(SyntaxKind::LEFT_CURLY);

    while !p.at(SyntaxKind::PIPE) && !p.at(SyntaxKind::RIGHT_CURLY) && !p.at_eof() {
        if p.at_in(names::RECORD_LABEL) {
            row_item(p);
            let ending = p.at_next(SyntaxKind::PIPE) || p.at_next(SyntaxKind::RIGHT_CURLY);
            if p.at(SyntaxKind::COMMA) && ending {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::PIPE) && !p.at(SyntaxKind::RIGHT_CURLY) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(TYPE_ROW_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in record");
        }
    }

    if p.at(SyntaxKind::PIPE) {
        row_tail(p);
    }

    p.expect(SyntaxKind::RIGHT_CURLY);
    m.end(p, SyntaxKind::TypeRecord);
}

fn type_effect_set(p: &mut Parser) {
    let mut m = p.start();

    p.expect(SyntaxKind::LEFT_SQUARE);
    while !p.at(SyntaxKind::PIPE) && !p.at(SyntaxKind::RIGHT_SQUARE) && !p.at_eof() {
        if p.at_in(TYPE_EFFECT_SET_MEMBER_START) {
            effect_set_member(p);
            let ending = p.at_next(SyntaxKind::PIPE) || p.at_next(SyntaxKind::RIGHT_SQUARE);
            if p.at(SyntaxKind::COMMA) && ending {
                p.error_recover("Trailing comma");
            } else if !p.at(SyntaxKind::PIPE) && !p.at(SyntaxKind::RIGHT_SQUARE) {
                p.expect(SyntaxKind::COMMA);
            }
        } else {
            if p.at_in(TYPE_EFFECT_SET_RECOVERY) {
                break;
            }
            p.error_recover("Unexpected token in effect set");
        }
    }

    if p.at(SyntaxKind::PIPE) {
        effect_set_tail(p);
    }

    p.expect(SyntaxKind::RIGHT_SQUARE);
    m.end(p, SyntaxKind::TypeEffectSet);
}

const TYPE_EFFECT_SET_RECOVERY: TokenSet = TokenSet::new(&[
    SyntaxKind::PIPE,
    SyntaxKind::RIGHT_SQUARE,
    SyntaxKind::LAYOUT_SEPARATOR,
    SyntaxKind::LAYOUT_END,
]);

const TYPE_EFFECT_SET_MEMBER_START: TokenSet =
    TYPE_ATOM_START.union(TokenSet::new(&[SyntaxKind::FORALL, SyntaxKind::HOLE]));

fn effect_set_member(p: &mut Parser) {
    let mut m = p.start();
    type_(p);
    m.end(p, SyntaxKind::TypeEffectSetMember);
}

fn effect_set_tail(p: &mut Parser) {
    let mut m = p.start();
    p.expect(SyntaxKind::PIPE);
    type_(p);
    m.end(p, SyntaxKind::TypeEffectSetTail);
}
