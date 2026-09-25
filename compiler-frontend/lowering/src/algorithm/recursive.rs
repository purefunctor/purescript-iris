use std::mem;
use std::sync::Arc;

use itertools::Itertools;
use petgraph::algo::tarjan_scc;
use rustc_hash::FxHashMap;
use smol_str::{SmolStr, StrExt};
use stabilizing::ExpectId;
use syntax::ast::{AstNode, support};
use syntax::{SyntaxKind, SyntaxToken, cst};

use crate::literal::{StringLiteral, decode_normal_string, decode_raw_string};
use crate::*;

use super::{Context, ItemGraph, LetBindingContext, State};

fn string_text(source: &str, token: SyntaxToken) -> Option<SmolStr> {
    string_literal_text(source, token)?.to_utf8().ok().map(SmolStr::from)
}

fn string_literal_text(source: &str, token: SyntaxToken) -> Option<StringLiteral> {
    let text = token.text(source);
    if text.starts_with("\"\"\"") {
        if text.len() >= 6 && text.ends_with("\"\"\"") {
            decode_raw_string(text)
        } else {
            Some(StringLiteral::from(text))
        }
    } else if text.starts_with('"') {
        if text.len() >= 2 && text.ends_with('"') {
            decode_normal_string(text)
        } else {
            Some(StringLiteral::from(text))
        }
    } else {
        Some(StringLiteral::from(text))
    }
}

fn string_literal(
    state: &mut State,
    source: &str,
    literal_source: StringLiteralSource,
    string: Option<SyntaxToken>,
    raw_string: Option<SyntaxToken>,
) -> (StringKind, Option<StringLiteral>) {
    if let Some(value) = string {
        let value = string_literal_text(source, value);
        if value.is_none() {
            state.errors.push(LoweringError::InvalidStringEscape { source: literal_source });
        }
        (StringKind::String, value)
    } else if let Some(value) = raw_string {
        (StringKind::RawString, string_literal_text(source, value))
    } else {
        (StringKind::String, None)
    }
}

fn integer_literal(text: &str, negative: bool) -> Option<i32> {
    let integer = if let Some(hex) = text.strip_prefix("0x") {
        let clean = hex.replace_smolstr("_", "");
        i32::from_str_radix(&clean, 16).ok()?
    } else {
        let clean = text.replace_smolstr("_", "");
        clean.parse().ok()?
    };

    if negative { Some(-integer) } else { Some(integer) }
}

fn number_literal(text: &str) -> SmolStr {
    text.replace_smolstr("_", "")
}

fn char_literal(text: &str) -> Option<char> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    if let Some(escaped) = inner.strip_prefix('\\') {
        match escaped {
            "n" => Some('\n'),
            "r" => Some('\r'),
            "t" => Some('\t'),
            "\\" => Some('\\'),
            "\"" => Some('"'),
            "'" => Some('\''),
            "0" => Some('\0'),
            escaped => {
                let hexadecimal = escaped.strip_prefix('x')?;
                let value = u32::from_str_radix(hexadecimal, 16).ok()?;
                char::from_u32(value)
            }
        }
    } else {
        let mut characters = inner.chars();
        let character = characters.next()?;
        characters.next().is_none().then_some(character)
    }
}

pub(crate) fn lower_binder(state: &mut State, context: &Context, cst: &cst::Binder) -> BinderId {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let kind = lower_binder_kind(state, context, cst, id);
    state.associate_binder_kind(id, kind);
    id
}

fn lower_binder_kind(
    state: &mut State,
    context: &Context<'_>,
    cst: &cst::Binder,
    id: BinderId,
) -> BinderKind {
    match cst {
        cst::Binder::BinderTyped(cst) => {
            let binder = cst.binder().map(|cst| lower_binder(state, context, &cst));
            let type_ = cst.type_().map(|cst| lower_type(state, context, &cst));
            BinderKind::Typed { binder, type_ }
        }
        cst::Binder::BinderOperatorChain(cst) => {
            let head = cst.binder().map(|cst| lower_binder(state, context, &cst));
            let tail = cst
                .children()
                .map(|cst| {
                    let operator = cst.operator();
                    let id = operator.and_then(|cst| lower_term_operator(state, context, &cst));
                    let element = cst.binder().map(|cst| lower_binder(state, context, &cst));
                    OperatorPair { id, element }
                })
                .collect();
            BinderKind::OperatorChain { head, tail }
        }
        cst::Binder::BinderInteger(cst) => {
            let value = cst.integer_token().and_then(|token| {
                integer_literal(token.text(context.source), cst.minus_token().is_some())
            });
            BinderKind::Integer { value }
        }
        cst::Binder::BinderNumber(cst) => {
            let negative = cst.minus_token().is_some();
            let value = cst.number_token().map(|token| number_literal(token.text(context.source)));
            BinderKind::Number { negative, value }
        }
        cst::Binder::BinderConstructor(cst) => {
            let resolution = cst.name().and_then(|cst| {
                let (qualifier, name) =
                    lower_qualified_name(context.source, &cst, cst::QualifiedName::upper)?;
                state.resolve_term_reference(context, qualifier.as_deref(), &name)
            });
            let arguments = cst.children().map(|cst| lower_binder(state, context, &cst)).collect();
            BinderKind::Constructor { resolution, arguments }
        }
        cst::Binder::BinderVariable(cst) => {
            let variable = cst.name_token().map(|cst| {
                let text = cst.text(context.source);
                SmolStr::from(text)
            });
            if let Some(name) = &variable {
                state.insert_binder(name, id);
            }
            BinderKind::Variable { variable }
        }
        cst::Binder::BinderNamed(cst) => {
            let named = cst.name_token().map(|cst| {
                let text = cst.text(context.source);
                SmolStr::from(text)
            });
            if let Some(name) = &named {
                state.insert_binder(name, id);
            }
            let binder = cst.binder().map(|cst| lower_binder(state, context, &cst));
            BinderKind::Named { named, binder }
        }
        cst::Binder::BinderWildcard(_) => BinderKind::Wildcard,
        cst::Binder::BinderString(cst) => {
            let source =
                StringLiteralSource::Binder(context.stabilized.lookup_cst(cst).expect_id());
            let (kind, value) =
                string_literal(state, context.source, source, cst.string(), cst.raw_string());
            BinderKind::String { kind, value }
        }
        cst::Binder::BinderChar(cst) => {
            let value = cst.char_token().and_then(|token| char_literal(token.text(context.source)));
            BinderKind::Char { value }
        }
        cst::Binder::BinderTrue(_) => BinderKind::Boolean { boolean: true },
        cst::Binder::BinderFalse(_) => BinderKind::Boolean { boolean: false },
        cst::Binder::BinderArray(cst) => {
            let array = cst.children().map(|cst| lower_binder(state, context, &cst)).collect();
            BinderKind::Array { array }
        }
        cst::Binder::BinderRecord(cst) => {
            let lower_item = |i| match i {
                cst::RecordItem::RecordField(cst) => {
                    let name = cst.name().and_then(|cst| {
                        let token = cst.text()?;
                        string_text(context.source, token)
                    });
                    let value = cst.binder().map(|cst| lower_binder(state, context, &cst));
                    BinderRecordItem::RecordField { name, value }
                }
                cst::RecordItem::RecordPun(cst) => {
                    let id = context.stabilized.lookup_cst(&cst).expect_id();

                    let name = cst.name().and_then(|cst| {
                        let token = cst.text()?;
                        string_text(context.source, token)
                    });

                    if let Some(name) = &name {
                        state.insert_record_pun(name, id);
                    }

                    BinderRecordItem::RecordPun { id, name }
                }
            };
            let record = cst.children().map(lower_item).collect();
            BinderKind::Record { record }
        }
        cst::Binder::BinderParenthesized(cst) => {
            let parenthesized = cst.binder().map(|cst| lower_binder(state, context, &cst));
            BinderKind::Parenthesized { parenthesized }
        }
    }
}

pub(crate) fn lower_expression(
    state: &mut State,
    context: &Context,
    cst: &cst::Expression,
) -> ExpressionId {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let kind = lower_expression_kind(state, context, cst);
    state.associate_expression_kind(id, kind);
    id
}

fn lower_expression_kind(
    state: &mut State,
    context: &Context<'_>,
    cst: &cst::Expression,
) -> ExpressionKind {
    match cst {
        cst::Expression::ExpressionTyped(cst) => {
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            let type_ = cst.signature().map(|cst| lower_type(state, context, &cst));
            ExpressionKind::Typed { expression, type_ }
        }
        cst::Expression::ExpressionOperatorChain(cst) => {
            let head = cst.expression().map(|cst| lower_expression(state, context, &cst));
            let tail = cst
                .children()
                .map(|cst| {
                    let operator = cst.operator();
                    let id = operator.and_then(|cst| lower_term_operator(state, context, &cst));
                    let expression = cst.expression();
                    let element = expression.map(|cst| lower_expression(state, context, &cst));
                    OperatorPair { id, element }
                })
                .collect();
            ExpressionKind::OperatorChain { head, tail }
        }
        cst::Expression::ExpressionInfixChain(cst) => {
            let head = cst.expression().map(|cst| lower_expression(state, context, &cst));
            let tail = cst
                .children()
                .map(|cst| {
                    let tick = cst.tick().and_then(|cst| {
                        let cst = cst.expression()?;
                        Some(lower_expression(state, context, &cst))
                    });
                    let element =
                        cst.expression().map(|cst| lower_expression(state, context, &cst));
                    InfixPair { tick, element }
                })
                .collect();
            ExpressionKind::InfixChain { head, tail }
        }
        cst::Expression::ExpressionNegate(cst) => {
            let negate = state.resolve_term_full(context, None, "negate");

            if negate.is_none() {
                let id = context.stabilized.lookup_cst(cst).expect_id();
                state.errors.push(LoweringError::NotInScope(NotInScope::NegateFn { id }));
            }

            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            ExpressionKind::Negate { negate, expression }
        }
        cst::Expression::ExpressionApplicationChain(cst) => {
            let lower_argument =
                |state: &mut State, context: &Context, cst: &cst::ExpressionArgument| match cst {
                    cst::ExpressionArgument::ExpressionTypeArgument(cst) => {
                        let id = cst.type_().map(|cst| lower_type(state, context, &cst));
                        ExpressionArgument::Type(id)
                    }
                    cst::ExpressionArgument::ExpressionTermArgument(cst) => {
                        let id = cst.expression().map(|cst| lower_expression(state, context, &cst));
                        ExpressionArgument::Term(id)
                    }
                };

            let function = cst.expression().map(|cst| lower_expression(state, context, &cst));
            let arguments =
                cst.children().map(|cst| lower_argument(state, context, &cst)).collect();

            ExpressionKind::Application { function, arguments }
        }
        cst::Expression::ExpressionIfThenElse(cst) => {
            let if_ = cst.if_().and_then(|cst| {
                let cst = cst.expression()?;
                Some(lower_expression(state, context, &cst))
            });
            let then = cst.then().and_then(|cst| {
                let cst = cst.expression()?;
                Some(lower_expression(state, context, &cst))
            });
            let else_ = cst.else_().and_then(|cst| {
                let cst = cst.expression()?;
                Some(lower_expression(state, context, &cst))
            });
            ExpressionKind::IfThenElse { if_, then, else_ }
        }
        cst::Expression::ExpressionLetIn(cst) => state.with_scope(|s| {
            let bindings = recover! { lower_bindings(s, context, &cst.bindings()?) };
            let expression = cst.expression().map(|cst| lower_expression(s, context, &cst));
            ExpressionKind::LetIn { bindings, expression }
        }),
        cst::Expression::ExpressionLambda(cst) => state.with_scope(|state| {
            state.push_binder_scope();
            let binders = recover! {
                cst.function_binders()?
                    .children()
                    .map(|cst| lower_binder(state, context, &cst))
                    .collect()
            };
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            ExpressionKind::Lambda { binders, expression }
        }),
        cst::Expression::ExpressionCaseOf(cst) => {
            fn lower_case_branch(
                state: &mut State,
                context: &Context,
                cst: &cst::CaseBranch,
            ) -> CaseBranch {
                state.with_scope(|state| {
                    state.push_binder_scope();
                    let binders = recover! {
                        cst.binders()?
                            .children()
                            .map(|cst| lower_binder(state, context, &cst))
                            .collect()
                    };
                    let guarded_expression =
                        cst.guarded_expression().map(|cst| lower_guarded(state, context, &cst));
                    CaseBranch { binders, guarded_expression }
                })
            }

            let trunk = recover! {
                cst.trunk()?
                    .children()
                    .map(|cst| lower_expression(state, context, &cst))
                    .collect()
            };
            let branches = recover! {
                cst.branches()?
                    .children()
                    .map(|cst| lower_case_branch(state, context, &cst))
                    .collect()
            };

            ExpressionKind::CaseOf { trunk, branches }
        }
        cst::Expression::ExpressionDo(cst) => state.with_scope(|state| {
            let qualifier = cst.qualifier().and_then(|cst| {
                let token = cst.text()?;
                let text = token.text(context.source).trim_end_matches('.');
                Some(SmolStr::from(text))
            });

            // Scan statements to determine which rebindable functions are needed:
            // - `bind` is needed if there's at least one `<-` statement
            // - `discard` is needed if there's a non-final discard statement
            let (has_bind, has_discard) = cst.statements().map_or((false, false), |statements| {
                let mut has_bind = false;
                let mut has_discard = false;

                for (position, statement) in statements.children().with_position() {
                    let is_final = position.is_last();
                    match statement {
                        cst::DoStatement::DoStatementBind(_) => has_bind = true,
                        cst::DoStatement::DoStatementDiscard(_) if !is_final => has_discard = true,
                        _ => {}
                    }
                }

                (has_bind, has_discard)
            });

            let mut resolve_do_fn = |kind: DoFn| {
                let name = match kind {
                    DoFn::Bind => "bind",
                    DoFn::Discard => "discard",
                };
                let resolution = state.resolve_term_full(context, qualifier.as_deref(), name);
                if resolution.is_none() {
                    let id = context.stabilized.lookup_cst(cst).expect_id();
                    state.errors.push(LoweringError::NotInScope(NotInScope::DoFn { kind, id }));
                }
                resolution
            };

            let bind = if has_bind { resolve_do_fn(DoFn::Bind) } else { None };
            let discard = if has_discard { resolve_do_fn(DoFn::Discard) } else { None };

            let statements = recover! {
                cst.statements()?
                    .children()
                    .map(|cst| lower_do_statement(state, context, &cst))
                    .collect()
            };

            ExpressionKind::Do { bind, discard, statements }
        }),
        cst::Expression::ExpressionAdo(cst) => state.with_scope(|state| {
            let qualifier = cst.qualifier().and_then(|cst| {
                let token = cst.text()?;
                let text = token.text(context.source).trim_end_matches('.');
                Some(SmolStr::from(text))
            });

            // Count action statements (Bind/Discard, ignoring Let) to determine
            // which rebindable functions are needed:
            // - 0 actions: only `pure` needed
            // - 1 action: only `map` needed
            // - 2+ actions: `map` and `apply` needed
            let action_count = cst.statements().map_or(0, |statements| {
                statements
                    .children()
                    .filter(|s| {
                        matches!(
                            s,
                            cst::DoStatement::DoStatementBind(_)
                                | cst::DoStatement::DoStatementDiscard(_)
                        )
                    })
                    .count()
            });

            let (needs_pure, needs_map, needs_apply) = match action_count {
                0 => (true, false, false),
                1 => (false, true, false),
                _ => (false, true, true),
            };

            let mut resolve_ado_fn = |kind: AdoFn| {
                let name = match kind {
                    AdoFn::Map => "map",
                    AdoFn::Apply => "apply",
                    AdoFn::Pure => "pure",
                };
                let resolution = state.resolve_term_full(context, qualifier.as_deref(), name);
                if resolution.is_none() {
                    let id = context.stabilized.lookup_cst(cst).expect_id();
                    state.errors.push(LoweringError::NotInScope(NotInScope::AdoFn { kind, id }));
                }
                resolution
            };

            let map = if needs_map { resolve_ado_fn(AdoFn::Map) } else { None };
            let apply = if needs_apply { resolve_ado_fn(AdoFn::Apply) } else { None };
            let pure = if needs_pure { resolve_ado_fn(AdoFn::Pure) } else { None };

            let outer_scope = state.graph_scope;
            let statements = recover! {
                cst.statements()?
                    .children()
                    .map(|cst| lower_ado_statement(state, context, outer_scope, &cst))
                    .collect()
            };
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));

            ExpressionKind::Ado { map, apply, pure, statements, expression }
        }),
        cst::Expression::ExpressionConstructor(cst) => {
            let resolution = cst.name().and_then(|cst| {
                let (qualifier, name) =
                    lower_qualified_name(context.source, &cst, cst::QualifiedName::upper)?;
                state.resolve_term_reference(context, qualifier.as_deref(), &name)
            });
            if resolution.is_none() {
                let id = context.stabilized.lookup_cst(cst).expect_id();
                state.errors.push(LoweringError::NotInScope(NotInScope::ExprConstructor { id }));
            }
            ExpressionKind::Constructor { resolution }
        }
        cst::Expression::ExpressionVariable(cst) => {
            let resolution = cst.name().and_then(|cst| {
                let (qualifier, name) =
                    lower_qualified_name(context.source, &cst, cst::QualifiedName::lower)?;
                state.resolve_term_full(context, qualifier.as_deref(), name.as_str())
            });
            if resolution.is_none() {
                let id = context.stabilized.lookup_cst(cst).expect_id();
                state.errors.push(LoweringError::NotInScope(NotInScope::ExprVariable { id }));
            }
            ExpressionKind::Variable { resolution }
        }
        cst::Expression::ExpressionOperatorName(cst) => {
            let resolution = cst.name().and_then(|cst| {
                let (qualifier, name) =
                    lower_qualified_name(context.source, &cst, cst::QualifiedName::operator_name)?;
                state.resolve_term_reference(context, qualifier.as_deref(), &name)
            });
            if resolution.is_none() {
                let id = context.stabilized.lookup_cst(cst).expect_id();
                state.errors.push(LoweringError::NotInScope(NotInScope::ExprOperatorName { id }));
            }
            ExpressionKind::OperatorName { resolution }
        }
        cst::Expression::ExpressionSection(_) => ExpressionKind::Section,
        cst::Expression::ExpressionHole(_) => ExpressionKind::Hole,
        cst::Expression::ExpressionString(cst) => {
            let string = support::token(cst.syntax(), SyntaxKind::STRING);
            let raw_string = support::token(cst.syntax(), SyntaxKind::RAW_STRING);
            let source =
                StringLiteralSource::Expression(context.stabilized.lookup_cst(cst).expect_id());
            let (kind, value) = string_literal(state, context.source, source, string, raw_string);
            ExpressionKind::String { kind, value }
        }
        cst::Expression::ExpressionChar(cst) => {
            let value = support::token(cst.syntax(), SyntaxKind::CHAR)
                .and_then(|token| char_literal(token.text(context.source)));
            ExpressionKind::Char { value }
        }
        cst::Expression::ExpressionTrue(_) => ExpressionKind::Boolean { boolean: true },
        cst::Expression::ExpressionFalse(_) => ExpressionKind::Boolean { boolean: false },
        cst::Expression::ExpressionInteger(cst) => {
            let value = support::token(cst.syntax(), SyntaxKind::INTEGER)
                .and_then(|token| integer_literal(token.text(context.source), false));
            ExpressionKind::Integer { value }
        }
        cst::Expression::ExpressionNumber(cst) => {
            let value = support::token(cst.syntax(), SyntaxKind::NUMBER)
                .map(|token| number_literal(token.text(context.source)));
            ExpressionKind::Number { value }
        }
        cst::Expression::ExpressionArray(cst) => {
            let array = cst.children().map(|cst| lower_expression(state, context, &cst)).collect();
            ExpressionKind::Array { array }
        }
        cst::Expression::ExpressionRecord(cst) => {
            let lower_item = |state: &mut State, cst| match cst {
                cst::RecordItem::RecordField(cst) => {
                    let name = cst.name().and_then(|cst| {
                        let token = cst.text()?;
                        string_text(context.source, token)
                    });
                    let value = cst.expression().map(|cst| lower_expression(state, context, &cst));
                    ExpressionRecordItem::RecordField { name, value }
                }
                cst::RecordItem::RecordPun(cst) => {
                    let id = context.stabilized.lookup_cst(&cst).expect_id();

                    let name = cst.name().and_then(|cst| {
                        let token = cst.text()?;
                        string_text(context.source, token)
                    });
                    let resolution = name.as_ref().and_then(|name| {
                        let qualifier: Option<&str> = None;
                        state.resolve_term_full(context, qualifier, name)
                    });

                    state.associate_record_pun(id, resolution);
                    ExpressionRecordItem::RecordPun { id, name, resolution }
                }
            };
            let record = cst.children().map(|cst| lower_item(state, cst)).collect();
            ExpressionKind::Record { record }
        }
        cst::Expression::ExpressionParenthesized(cst) => {
            let parenthesized = cst.expression().map(|cst| lower_expression(state, context, &cst));
            ExpressionKind::Parenthesized { parenthesized }
        }
        cst::Expression::ExpressionRecordAccess(cst) => {
            let record = cst.expression().map(|cst| lower_expression(state, context, &cst));
            let labels = cst
                .children()
                .map(|cst| {
                    let id = context.stabilized.lookup_cst(&cst).expect_id();
                    let token = cst.name()?.text()?;
                    let name = string_text(context.source, token)?;
                    Some(RecordAccessLabel { id, name })
                })
                .collect();
            ExpressionKind::RecordAccess { record, labels }
        }
        cst::Expression::ExpressionRecordUpdate(cst) => {
            let record = cst.expression().map(|cst| lower_expression(state, context, &cst));
            let updates = recover! { lower_record_updates(state, context, &cst.record_updates()?) };
            ExpressionKind::RecordUpdate { record, updates }
        }
    }
}

fn lower_guarded(
    state: &mut State,
    context: &Context,
    cst: &cst::GuardedExpression,
) -> GuardedExpression {
    match cst {
        cst::GuardedExpression::Unconditional(cst) => {
            let where_expression =
                cst.where_expression().map(|cst| lower_where_expression(state, context, &cst));
            GuardedExpression::Unconditional { where_expression }
        }
        cst::GuardedExpression::Conditionals(cst) => {
            let pattern_guarded =
                cst.children().map(|cst| lower_pattern_guarded(state, context, &cst)).collect();
            GuardedExpression::Conditionals { pattern_guarded }
        }
    }
}

fn lower_where_expression(
    state: &mut State,
    context: &Context,
    cst: &cst::WhereExpression,
) -> WhereExpression {
    state.with_scope(|s| {
        let bindings = recover! { lower_bindings(s, context, &cst.bindings()?) };
        let expression = cst.expression().map(|cst| lower_expression(s, context, &cst));
        WhereExpression { expression, bindings }
    })
}

fn lower_pattern_guarded(
    state: &mut State,
    context: &Context,
    cst: &cst::PatternGuarded,
) -> PatternGuarded {
    // ```
    // guarded a b
    //   | Just c <- a = c
    //   | Just d <- b = d
    // ```
    state.with_scope(|s| {
        let pattern_guards =
            cst.children().map(|cst| lower_pattern_guard(s, context, &cst)).collect();
        let where_expression =
            cst.where_expression().map(|cst| lower_where_expression(s, context, &cst));
        PatternGuarded { pattern_guards, where_expression }
    })
}

fn lower_pattern_guard(
    state: &mut State,
    context: &Context,
    cst: &cst::PatternGuard,
) -> PatternGuard {
    match cst {
        cst::PatternGuard::PatternGuardBinder(cst) => {
            state.push_binder_scope();
            let binder = cst.binder().map(|cst| lower_binder(state, context, &cst));
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            PatternGuard { binder, expression }
        }
        cst::PatternGuard::PatternGuardExpression(cst) => {
            let binder = None;
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            PatternGuard { binder, expression }
        }
    }
}

fn lower_bindings(
    state: &mut State,
    context: &Context,
    cst: &cst::LetBindingStatements,
) -> Arc<[LetBindingChunk]> {
    #[derive(Debug, PartialEq, Eq)]
    enum Chunk {
        Pattern,
        Equation,
    }

    let chunks = cst.children().chunk_by(|cst| match cst {
        cst::LetBinding::LetBindingPattern(_) => Chunk::Pattern,
        cst::LetBinding::LetBindingSignature(_) => Chunk::Equation,
        cst::LetBinding::LetBindingEquation(_) => Chunk::Equation,
    });

    let mut result = vec![];
    for (kind, children) in chunks.into_iter() {
        match kind {
            Chunk::Pattern => {
                // Each pattern becomes its own chunk
                for pattern in children {
                    result.push(lower_pattern_chunk(state, context, pattern));
                }
            }
            Chunk::Equation => {
                // All consecutive equations become one Names chunk
                result.push(lower_equation_chunk(state, context, children));
            }
        }
    }

    result.into()
}

fn lower_pattern_chunk(
    state: &mut State,
    context: &Context,
    pattern: cst::LetBinding,
) -> LetBindingChunk {
    let cst::LetBinding::LetBindingPattern(pattern) = &pattern else {
        unreachable!("invariant violated: expected LetBindingPattern");
    };
    let source = context.stabilized.lookup_cst(pattern).expect_id();
    let where_expression =
        pattern.where_expression().map(|cst| lower_where_expression(state, context, &cst));
    state.push_binder_scope();
    let binder = pattern.binder().map(|cst| lower_binder(state, context, &cst));
    LetBindingChunk::Pattern { source, binder, where_expression }
}

struct PendingLetBinding {
    name: Option<SmolStr>,
    signature: Option<LetBindingSignatureId>,
    equations: Arc<[LetBindingEquationId]>,
}

fn lower_equation_chunk(
    state: &mut State,
    context: &Context,
    children: impl Iterator<Item = cst::LetBinding>,
) -> LetBindingChunk {
    let children = children.chunk_by(|cst| match cst {
        cst::LetBinding::LetBindingPattern(_) => {
            unreachable!("invariant violated: expected LetBindingSignature / LetBindingEquation");
        }
        cst::LetBinding::LetBindingSignature(cst) => cst.name_token().map(|cst| {
            let text = cst.text(context.source);
            SmolStr::from(text)
        }),
        cst::LetBinding::LetBindingEquation(cst) => cst.name_token().map(|cst| {
            let text = cst.text(context.source);
            SmolStr::from(text)
        }),
    });

    let mut pending = vec![];

    for (name, mut children) in children.into_iter() {
        let mut signature = None;
        let mut equations = vec![];

        if let Some(cst) = children.next() {
            match cst {
                cst::LetBinding::LetBindingPattern(_) => {
                    unreachable!(
                        "invariant violated: expected LetBindingSignature / LetBindingEquation"
                    );
                }
                cst::LetBinding::LetBindingSignature(cst) => {
                    let ast_id = context.stabilized.lookup_cst(&cst).expect_id();
                    signature = Some(ast_id);
                }
                cst::LetBinding::LetBindingEquation(cst) => {
                    let ast_id = context.stabilized.lookup_cst(&cst).expect_id();
                    equations.push(ast_id);
                }
            }
        }

        children.for_each(|cst| {
            if let cst::LetBinding::LetBindingEquation(cst) = cst {
                let ast_id = context.stabilized.lookup_cst(&cst).expect_id();
                equations.push(ast_id);
            }
        });

        let equations = equations.into();
        pending.push(PendingLetBinding { name, signature, equations });
    }

    let mut let_bound = FxHashMap::default();
    let mut groups = vec![];

    for PendingLetBinding { name, signature, equations } in pending {
        let group = LetBindingNameGroup { name: name.clone(), signature, equations };

        let id = state.alloc_let_binding(group);

        if let Some(name) = name {
            let_bound.insert(name, id);
        }

        groups.push(id);
    }

    let graph_node =
        state.graph.inner.alloc(GraphNode::Let { parent: state.graph_scope, bindings: let_bound });

    state.graph_scope = Some(graph_node);
    state.let_binding_contexts.push(LetBindingContext {
        scope: graph_node,
        current_binding: None,
        dependencies: ItemGraph::default(),
    });

    for &id in &groups {
        let dependency_context = state
            .let_binding_contexts
            .last_mut()
            .expect("invariant violated: expected let binding dependency context");
        dependency_context.current_binding = Some(id);

        let let_binding = &state.tree.let_binding_groups[id];
        let signature = let_binding.signature;
        let equations = Arc::clone(&let_binding.equations);

        state.with_scope(|state| {
            let signature = signature.and_then(|id| {
                let cst = context.stabilized.ast_ptr(id)?.try_to_node(context.root)?;
                let cst = cst.type_()?;
                Some(lower_forall(state, context, &cst))
            });

            let equations = equations.iter().filter_map(|&id| {
                let cst = context.stabilized.ast_ptr(id)?.try_to_node(context.root)?;
                Some(lower_equation_like(
                    state,
                    context,
                    None,
                    cst,
                    cst::LetBindingEquation::function_binders,
                    cst::LetBindingEquation::guarded_expression,
                ))
            });

            let equations = equations.collect();

            let info = LetBindingName { signature, equations };
            state.associate_let_binding_name(id, info);
        });

        let dependency_context = state
            .let_binding_contexts
            .last_mut()
            .expect("invariant violated: expected let binding dependency context");
        dependency_context.current_binding = None;
    }

    // Compute SCCs from dependency graph
    let dependency_context = state
        .let_binding_contexts
        .pop()
        .expect("invariant violated: expected let binding dependency context");
    let mut graph = dependency_context.dependencies;
    for &id in &groups {
        graph.add_node(id);
    }

    let sccs = tarjan_scc(&graph);
    let scc = sccs
        .into_iter()
        .map(|scc| match scc[..] {
            [single] if !graph.contains_edge(single, single) => Scc::Base(single),
            [single] => Scc::Recursive(single),
            _ => Scc::Mutual(scc),
        })
        .collect();

    LetBindingChunk::Names { bindings: groups.into(), scc }
}

pub(crate) fn lower_equation_like<T: AstNode>(
    state: &mut State,
    context: &Context,
    source: Option<indexing::EquationSourceId>,
    equation: T,
    binders: impl Fn(&T) -> Option<cst::FunctionBinders>,
    guarded: impl Fn(&T) -> Option<cst::GuardedExpression>,
) -> Equation {
    state.with_scope(|state| {
        state.push_binder_scope();
        let binders = recover! {
            binders(&equation)?
                .children()
                .map(|cst| lower_binder(state, context, &cst))
                .collect()
        };
        let guarded = guarded(&equation).map(|cst| lower_guarded(state, context, &cst));
        Equation { source, binders, guarded }
    })
}

fn lower_do_statement(
    state: &mut State,
    context: &Context,
    cst: &cst::DoStatement,
) -> DoStatementId {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let statement = match cst {
        cst::DoStatement::DoStatementBind(cst) => {
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            state.push_binder_scope();
            let binder = cst.binder().map(|cst| lower_binder(state, context, &cst));
            DoStatement::Bind { binder, expression }
        }
        cst::DoStatement::DoStatementLet(cst) => {
            let statements = recover! { lower_bindings(state, context, &cst.statements()?) };
            DoStatement::Let { statements }
        }
        cst::DoStatement::DoStatementDiscard(cst) => {
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            DoStatement::Discard { expression }
        }
    };
    state.associate_do_statement(id, statement);
    id
}

fn lower_ado_statement(
    state: &mut State,
    context: &Context,
    outer_scope: Option<GraphNodeId>,
    cst: &cst::DoStatement,
) -> DoStatementId {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let statement = match cst {
        cst::DoStatement::DoStatementBind(cst) => {
            let current_scope = mem::replace(&mut state.graph_scope, outer_scope);
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            state.graph_scope = current_scope;

            state.push_binder_scope();
            let binder = cst.binder().map(|cst| lower_binder(state, context, &cst));

            DoStatement::Bind { binder, expression }
        }
        cst::DoStatement::DoStatementLet(cst) => {
            let statements = recover! {
                let statements = cst.statements()?;
                lower_bindings(state, context, &statements)
            };
            DoStatement::Let { statements }
        }
        cst::DoStatement::DoStatementDiscard(cst) => {
            let current_scope = mem::replace(&mut state.graph_scope, outer_scope);
            let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
            state.graph_scope = current_scope;

            DoStatement::Discard { expression }
        }
    };
    state.associate_do_statement(id, statement);
    id
}

fn lower_record_updates(
    state: &mut State,
    context: &Context,
    cst: &cst::RecordUpdates,
) -> Arc<[RecordUpdate]> {
    cst.children()
        .map(|cst| match cst {
            cst::RecordUpdate::RecordUpdateLeaf(cst) => {
                let name = cst.name().and_then(|cst| {
                    let token = cst.text()?;
                    string_text(context.source, token)
                });
                let expression = cst.expression().map(|cst| lower_expression(state, context, &cst));
                RecordUpdate::Leaf { name, expression }
            }
            cst::RecordUpdate::RecordUpdateBranch(cst) => {
                let name = cst.name().and_then(|cst| {
                    let token = cst.text()?;
                    string_text(context.source, token)
                });
                let updates =
                    recover! { lower_record_updates(state, context, &cst.record_updates()?) };
                RecordUpdate::Branch { name, updates }
            }
        })
        .collect()
}

pub(crate) fn lower_type(state: &mut State, context: &Context, cst: &cst::Type) -> TypeId {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let kind = lower_type_kind(state, context, cst, id);
    state.associate_type_kind(id, kind);
    id
}

fn lower_type_kind(
    state: &mut State,
    context: &Context<'_>,
    cst: &cst::Type,
    id: TypeId,
) -> TypeKind {
    match cst {
        cst::Type::TypeApplicationChain(cst) => {
            let mut children = cst.children();
            let function = children.next().map(|cst| lower_type(state, context, &cst));
            let in_constraint = mem::replace(&mut state.in_constraint, false);
            let arguments = children.map(|cst| lower_type(state, context, &cst)).collect();
            state.in_constraint = in_constraint;
            TypeKind::ApplicationChain { function, arguments }
        }
        cst::Type::TypeArrow(cst) => {
            let mut children = cst.children().map(|cst| lower_type(state, context, &cst));
            let argument = children.next();
            let result = children.next();
            TypeKind::Arrow { argument, result }
        }
        cst::Type::TypeConstrained(cst) => {
            let mut children = cst.children();
            let in_constraint = mem::replace(&mut state.in_constraint, true);
            let constraint = children.next().map(|cst| lower_type(state, context, &cst));
            state.in_constraint = in_constraint;
            let constrained = children.next().map(|cst| lower_type(state, context, &cst));
            TypeKind::Constrained { constraint, constrained }
        }
        cst::Type::TypeConstructor(cst) => {
            let resolution = cst.name().and_then(|cst| {
                let (qualifier, name) =
                    lower_qualified_name(context.source, &cst, cst::QualifiedName::upper)?;
                if state.in_constraint {
                    state.resolve_class_reference(context, qualifier.as_deref(), &name).or_else(
                        || state.resolve_type_reference(context, qualifier.as_deref(), &name),
                    )
                } else {
                    state.resolve_type_reference(context, qualifier.as_deref(), &name)
                }
            });
            if resolution.is_none() {
                let id = context.stabilized.lookup_cst(cst).expect_id();
                state.errors.push(LoweringError::NotInScope(NotInScope::TypeConstructor { id }));
            }
            TypeKind::Constructor { resolution }
        }
        // Rank-N Types must be scoped. See `lower_forall`.
        cst::Type::TypeForall(cst) => state.with_scope(|s| {
            s.push_forall_scope();
            let bindings = cst
                .children()
                .map(|cst| lower_type_variable_binding(s, context, &cst, false))
                .collect();
            let inner = cst.type_().map(|cst| lower_type(s, context, &cst));
            TypeKind::Forall { bindings, inner }
        }),
        cst::Type::TypeHole(_) => TypeKind::Hole,
        cst::Type::TypeInteger(cst) => {
            let value = cst.integer_token().and_then(|token| {
                integer_literal(token.text(context.source), cst.minus_token().is_some())
            });
            TypeKind::Integer { value }
        }
        cst::Type::TypeKinded(cst) => {
            let mut children = cst.children().map(|cst| lower_type(state, context, &cst));
            let type_ = children.next();
            let kind = children.next();
            TypeKind::Kinded { type_, kind }
        }
        cst::Type::TypeOperatorName(cst) => {
            let qualified = cst.name().and_then(|cst| {
                lower_qualified_name(context.source, &cst, cst::QualifiedName::operator_name)
            });

            const FUNCTION_ARROW: &str = "->";

            // The function arrow `->` is treated as a reserved operator by the
            // compiler; we cannot create an operator declaration for it in the
            // Prim module; subsequently, it has no TypeItemId and cannot be
            // resolved like TypeKind::Operator.
            //
            // In light of this, we desugar the function arrow into the `Function`
            // constructor, which does have a TypeItemId and can be resolved as a
            // TypeKind::Constructor.
            if let Some((qualifier, name)) = &qualified
                && name == FUNCTION_ARROW
            {
                let resolution =
                    state.resolve_type_reference(context, qualifier.as_deref(), "Function");
                if resolution.is_none() {
                    let id = context.stabilized.lookup_cst(cst).expect_id();
                    state
                        .errors
                        .push(LoweringError::NotInScope(NotInScope::TypeOperatorName { id }));
                }
                TypeKind::Constructor { resolution }
            } else {
                let resolution = qualified.and_then(|(qualifier, name)| {
                    state.resolve_type_reference(context, qualifier.as_deref(), &name)
                });
                if resolution.is_none() {
                    let id = context.stabilized.lookup_cst(cst).expect_id();
                    state
                        .errors
                        .push(LoweringError::NotInScope(NotInScope::TypeOperatorName { id }));
                }
                TypeKind::Operator { resolution }
            }
        }
        cst::Type::TypeOperatorChain(cst) => {
            let head = cst.type_().map(|cst| lower_type(state, context, &cst));
            let tail = cst
                .children()
                .map(|cst| {
                    let operator = cst.operator();
                    let id = operator.and_then(|cst| lower_type_operator(state, context, &cst));
                    let element = cst.type_().as_ref().map(|cst| lower_type(state, context, cst));
                    OperatorPair { id, element }
                })
                .collect();
            TypeKind::OperatorChain { head, tail }
        }
        cst::Type::TypeString(cst) => {
            let source = StringLiteralSource::Type(context.stabilized.lookup_cst(cst).expect_id());
            let (kind, value) =
                string_literal(state, context.source, source, cst.string(), cst.raw_string());
            TypeKind::String { kind, value }
        }
        cst::Type::TypeVariable(cst) => {
            let name = cst.name_token().map(|cst| {
                let text = cst.text(context.source);
                SmolStr::from(text)
            });
            let resolution = cst.name_token().and_then(|cst| {
                let text = cst.text(context.source);
                state.resolve_type_variable(id, text)
            });
            if resolution.is_none() {
                let id = context.stabilized.lookup_cst(cst).expect_id();
                state.errors.push(LoweringError::NotInScope(NotInScope::TypeVariable { id }));
            }
            TypeKind::Variable { name, resolution }
        }
        cst::Type::TypeWildcard(_) => TypeKind::Wildcard,
        cst::Type::TypeRecord(cst) => {
            let items = cst.children().map(|cst| lower_row_item(state, context, &cst)).collect();
            let tail = cst.tail().and_then(|cst| {
                let cst = cst.type_()?;
                Some(lower_type(state, context, &cst))
            });
            TypeKind::Record { items, tail }
        }
        cst::Type::TypeRow(cst) => {
            let items = cst.children().map(|cst| lower_row_item(state, context, &cst)).collect();
            let tail = cst.tail().and_then(|cst| {
                let cst = cst.type_()?;
                Some(lower_type(state, context, &cst))
            });
            TypeKind::Row { items, tail }
        }
        cst::Type::TypeParenthesized(cst) => {
            let parenthesized = cst.type_().map(|cst| lower_type(state, context, &cst));
            TypeKind::Parenthesized { parenthesized }
        }
    }
}

pub(crate) fn lower_forall(state: &mut State, context: &Context, cst: &cst::Type) -> TypeId {
    // To enable lexically-scoped type variables, we avoid calling `with_scope`
    // as to not reset to the parent scope once all the top-level `forall` have
    // been lowered. In `lower_type`, the `TypeForall` branch explicitly calls
    // `with_scope` in order to scope Rank-N type variables.
    if let cst::Type::TypeForall(f) = cst {
        let id = context.stabilized.lookup_cst(cst).expect_id();
        state.push_forall_scope();
        let bindings = f
            .children()
            .map(|cst| lower_type_variable_binding(state, context, &cst, false))
            .collect();
        let inner = f.type_().map(|cst| lower_forall(state, context, &cst));
        let kind = TypeKind::Forall { bindings, inner };
        state.associate_type_kind(id, kind);
        id
    } else {
        lower_type(state, context, cst)
    }
}

fn lower_term_operator(
    state: &mut State,
    context: &Context,
    cst: &cst::TermOperator,
) -> Option<TermOperatorId> {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let (qualifier, name) = cst
        .qualified()
        .and_then(|cst| lower_qualified_name(context.source, &cst, cst::QualifiedName::operator))?;

    let Some((file_id, term_id)) =
        state.resolve_term_reference(context, qualifier.as_deref(), &name)
    else {
        let id = context.stabilized.lookup_cst(cst).expect_id();
        state.errors.push(LoweringError::NotInScope(NotInScope::TermOperator { id }));
        return None;
    };

    state.tree.term_operators.insert(id, (file_id, term_id));

    Some(id)
}

fn lower_type_operator(
    state: &mut State,
    context: &Context,
    cst: &cst::TypeOperator,
) -> Option<TypeOperatorId> {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let (qualifier, name) = cst
        .qualified()
        .and_then(|cst| lower_qualified_name(context.source, &cst, cst::QualifiedName::operator))?;

    let Some((file_id, type_id)) =
        state.resolve_type_reference(context, qualifier.as_deref(), &name)
    else {
        let id = context.stabilized.lookup_cst(cst).expect_id();
        state.errors.push(LoweringError::NotInScope(NotInScope::TypeOperator { id }));
        return None;
    };

    state.tree.type_operators.insert(id, (file_id, type_id));

    Some(id)
}

pub(crate) fn lower_qualified_name(
    source: &str,
    cst: &cst::QualifiedName,
    token: impl Fn(&cst::QualifiedName) -> Option<SyntaxToken>,
) -> Option<(Option<SmolStr>, SmolStr)> {
    let qualifier = cst.qualifier().and_then(|cst| {
        let token = cst.text()?;
        let text = token.text(source).trim_end_matches('.');
        Some(SmolStr::from(text))
    });

    let token = token(cst)?;
    let text = token.text(source).trim_start_matches('(').trim_end_matches(')');
    let name = SmolStr::from(text);

    Some((qualifier, name))
}

pub(crate) fn lower_type_variable_binding(
    state: &mut State,
    context: &Context,
    cst: &cst::TypeVariableBinding,
    default_visible: bool,
) -> TypeVariableBinding {
    let id = context.stabilized.lookup_cst(cst).expect_id();
    let visible = cst.at().is_some() || default_visible;
    let name = cst.name().map(|cst| {
        let text = cst.text(context.source);
        SmolStr::from(text)
    });
    let kind = cst.kind().map(|cst| lower_type(state, context, &cst));
    if let Some(name) = &name {
        state.insert_bound_variable(name, id);
    }
    TypeVariableBinding { visible, id, name, kind }
}

fn lower_row_item(state: &mut State, context: &Context, cst: &cst::TypeRowItem) -> TypeRowItem {
    let name = cst.name().and_then(|cst| {
        let token = cst.text()?;
        string_text(context.source, token)
    });
    let type_ = cst.type_().map(|t| lower_type(state, context, &t));
    TypeRowItem { name, type_ }
}
