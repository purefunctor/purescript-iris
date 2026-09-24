use std::sync::Arc;

use checking::evidence::Evidence;
use checking::tree as checking_tree;
use indexing::{DeriveItemId, IndexedTermItemKind, InstanceItemId, TermItemId};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use smol_str::{SmolStr, format_smolstr};

use crate::error::UnsupportedState;
use crate::tree::{
    Binding, CaseAlternative, Declaration, DeclarationKind, ExpressionId, ExpressionKind, Global,
    GlobalId, Guard, GuardedAlternative, InstanceIdentity, PatternId, PatternKind, RecordField,
};

use super::evidence::evidence_variable;
use super::expression::{convert_expression, convert_pattern, patterns};
use super::{Context, ConversionResult};

pub(super) fn term_declaration(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    term_id: TermItemId,
    exported: bool,
) -> ConversionResult<Option<Declaration>> {
    let indexed_module = Arc::clone(&context.indexed);
    let checked = Arc::clone(&context.checked);
    let indexed = &indexed_module.items[term_id];
    if matches!(indexed.kind, IndexedTermItemKind::Operator { .. }) {
        return Ok(None);
    }
    let item_name = match &indexed.name {
        Some(name) => SmolStr::clone(name),
        None => context.term_fallback(term_id),
    };
    let global = Global { id: GlobalId::Term(context.file_id, term_id), item_name };
    let recursive_group = context.recursive_groups.get(&term_id).copied();
    if matches!(indexed.kind, IndexedTermItemKind::ClassMember { .. }) {
        let kind = DeclarationKind::Value(class_member_selector(context, term_id)?);
        return Ok(Some(Declaration { global, exported, recursive_group, kind }));
    }
    let declaration_id = checked
        .tree
        .lookup_term(term_id)
        .ok_or_else(|| context.unsupported(UnsupportedState::MissingTermDeclaration(term_id)))?;
    let declaration = &checked.tree[declaration_id];
    let kind = match &declaration.kind {
        checking_tree::TermDeclarationKind::Value(value) => {
            DeclarationKind::Value(value_declaration(context, value)?)
        }
        checking_tree::TermDeclarationKind::Foreign => DeclarationKind::Foreign,
        checking_tree::TermDeclarationKind::Constructor(constructor) => {
            if context.constructor_is_newtype(context.file_id, term_id)? {
                return Ok(None);
            }
            DeclarationKind::Constructor { arity: constructor.arguments.len() }
        }
        checking_tree::TermDeclarationKind::Instance(_) => {
            unreachable!("invariant violated: instance stored as a term declaration")
        }
    };
    Ok(Some(Declaration { global, exported, recursive_group, kind }))
}

fn class_member_selector(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    term_id: TermItemId,
) -> ConversionResult<ExpressionId> {
    let dictionary = context.fresh_parameter("dictionary".into())?;
    let record = context.expression(ExpressionKind::Local { parameter: dictionary.clone() });
    let field = context.member_field((context.file_id, term_id))?;
    let body = context.expression(ExpressionKind::Project { record, field });
    Ok(context.parameter_abstraction([dictionary], body))
}

pub(super) fn instance_declaration(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    item_id: InstanceItemId,
) -> ConversionResult<Declaration> {
    let indexed = &context.indexed.items[item_id];
    let identity = InstanceIdentity::Declared(context.file_id, indexed.id);
    let declaration_id = context
        .checked
        .tree
        .lookup_instance(item_id)
        .ok_or_else(|| context.unsupported(UnsupportedState::MissingInstanceDeclaration))?;
    convert_instance_declaration(context, identity, declaration_id)
}

pub(super) fn derive_declaration(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    item_id: DeriveItemId,
) -> ConversionResult<Declaration> {
    let indexed = &context.indexed.items[item_id];
    let identity = InstanceIdentity::Derived(context.file_id, indexed.id);
    let declaration_id = context
        .checked
        .tree
        .lookup_derive(item_id)
        .ok_or_else(|| context.unsupported(UnsupportedState::MissingInstanceDeclaration))?;
    convert_instance_declaration(context, identity, declaration_id)
}

fn convert_instance_declaration(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    identity: InstanceIdentity,
    declaration_id: checking_tree::TermDeclarationId,
) -> ConversionResult<Declaration> {
    let checked = Arc::clone(&context.checked);
    let declaration = &checked.tree[declaration_id];
    let checking_tree::TermDeclarationKind::Instance(instance) = &declaration.kind else {
        unreachable!("invariant violated: instance identity has non-instance declaration")
    };

    let parameters = instance.evidences.iter().map(|evidence| match evidence.evidence {
        Evidence::Given(binder) => context.evidence_parameter(binder),
        _ => Err(context.unsupported(UnsupportedState::InvalidInstancePrerequisite)),
    });
    let parameters = parameters.collect::<ConversionResult<Vec<_>>>()?;

    let body = context.evidence_scope(|context| match &instance.implementation {
        checking_tree::InstanceImplementation::Delegate { constraint, evidence } => {
            evidence_variable(context, *evidence, Some(*constraint))
        }
        checking_tree::InstanceImplementation::Members(members) => {
            let mut fields = Vec::new();
            for superclass in instance.superclasses.iter() {
                let expression =
                    evidence_variable(context, superclass.evidence, Some(superclass.constraint))?;
                let expression = context.expression(ExpressionKind::Abstraction {
                    parameters: Arc::from([]),
                    body: expression,
                });
                let field = context.superclass_field(superclass.id)?;
                fields.push(RecordField { field, expression });
            }
            for member in members.iter() {
                let expression = member_declaration(context, member)?;
                let field = context.member_field(member.resolution)?;
                fields.push(RecordField { field, expression });
            }
            Ok(context.expression(ExpressionKind::Record { fields: fields.into() }))
        }
    })?;
    let value = context.parameter_abstraction(parameters, body);
    let item_name = context.instance_name(identity)?;
    let global = Global { id: GlobalId::Instance(identity), item_name };
    let kind = DeclarationKind::Value(value);
    Ok(Declaration { global, exported: true, recursive_group: None, kind })
}

fn member_declaration(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    member: &checking_tree::InstanceMember,
) -> ConversionResult<ExpressionId> {
    let value = checking_tree::ValueDeclaration {
        abstractions: Arc::clone(&member.abstractions),
        equations: Arc::clone(&member.equations),
    };
    value_declaration(context, &value)
}

fn value_declaration(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    value: &checking_tree::ValueDeclaration,
) -> ConversionResult<ExpressionId> {
    let body = equations(context, &value.equations)?;
    declaration_abstraction(context, &value.abstractions, body)
}

fn equations(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    equations: &[checking_tree::Equation],
) -> ConversionResult<ExpressionId> {
    let Some(first) = equations.first() else {
        return Err(context.unsupported(UnsupportedState::MissingEquation));
    };
    if equations.len() == 1 {
        let body = context
            .evidence_scope(|context| guarded_expression(context, &first.guarded_expression))?;
        let patterns = function_patterns(context, &first.binders)?;
        return Ok(context.abstraction(patterns, body));
    }

    let arity = equations.iter().map(|equation| equation.binders.len()).max().unwrap_or(0);
    let mut parameter_patterns = Vec::with_capacity(arity);
    let mut scrutinees = Vec::with_capacity(arity);
    for position in 0..arity {
        let fallback = format_smolstr!("argument{position}");
        let type_id = equations
            .iter()
            .find_map(|equation| equation.binders.get(position))
            .map(|&binder| context.checked.tree[binder].type_id);
        let name = match type_id {
            Some(type_id) => context.type_parameter_name(type_id, fallback)?,
            None => fallback,
        };
        let parameter = context.fresh_parameter(name)?;
        let pattern = context.pattern(PatternKind::Variable(parameter.clone()));
        let scrutinee = context.expression(ExpressionKind::Local { parameter: parameter.clone() });
        parameter_patterns.push(pattern);
        scrutinees.push(scrutinee);
    }

    let body = context.evidence_scope(|context| {
        let mut alternatives = Vec::with_capacity(equations.len());
        for equation in equations {
            let mut patterns = patterns(context, &equation.binders)?;
            let supplied = patterns.len();
            while patterns.len() < arity {
                patterns.push(context.pattern(PatternKind::Wildcard));
            }
            let expression = guarded_expression(context, &equation.guarded_expression)?;
            let remaining_arguments = scrutinees.iter().skip(supplied).copied();
            let expression = context.application(expression, remaining_arguments)?;
            alternatives.push(CaseAlternative { patterns: patterns.into(), expression });
        }
        Ok(context.expression(ExpressionKind::Case {
            scrutinees: scrutinees.into(),
            alternatives: alternatives.into(),
        }))
    })?;
    Ok(context.abstraction(parameter_patterns, body))
}

fn declaration_abstraction(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    abstractions: &[checking_tree::DeclarationAbstraction],
    mut body: ExpressionId,
) -> ConversionResult<ExpressionId> {
    // Equations and generated member bodies already supply the explicit lambdas.
    // Open only the prefix containing evidence so `A -> C => B` becomes
    // `\argument -> \dictionary -> body`, preserving the remaining lambdas.
    let Some(last_evidence) = abstractions.iter().rposition(|abstraction| {
        matches!(abstraction, checking_tree::DeclarationAbstraction::Evidence { .. })
    }) else {
        return Ok(body);
    };
    let runtime_abstractions = abstractions[..=last_evidence].iter().filter(|abstraction| {
        !matches!(abstraction, checking_tree::DeclarationAbstraction::Type { .. })
    });
    let groups = runtime_abstractions.chunk_by(|abstraction| {
        matches!(abstraction, checking_tree::DeclarationAbstraction::Evidence { .. })
    });
    let mut parameter_groups = vec![];
    for (_, group) in &groups {
        let mut parameters = vec![];
        for abstraction in group {
            match abstraction {
                checking_tree::DeclarationAbstraction::Evidence { binder, .. } => {
                    let parameter = context.evidence_parameter(*binder)?;
                    parameters.push(context.pattern(PatternKind::Variable(parameter)));
                }
                checking_tree::DeclarationAbstraction::Argument => {
                    let ExpressionKind::Abstraction { parameters: arguments, body: inner } =
                        &context.storage[body].kind
                    else {
                        unreachable!("invariant violated: declaration argument has no lambda")
                    };
                    let (argument, remaining) = arguments
                        .split_first()
                        .expect("invariant violated: declaration argument has an empty lambda");
                    parameters.push(*argument);
                    body = context.abstraction(remaining.to_vec(), *inner);
                }
                checking_tree::DeclarationAbstraction::Type { .. } => unreachable!(),
            }
        }
        parameter_groups.push(parameters);
    }
    for parameters in parameter_groups.into_iter().rev() {
        body = context.abstraction(parameters, body);
    }
    Ok(body)
}

pub(super) fn function_patterns(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    binders: &[checking_tree::BinderId],
) -> ConversionResult<Vec<PatternId>> {
    let mut converted = Vec::with_capacity(binders.len());
    for (position, &binder) in binders.iter().enumerate() {
        let pattern = convert_pattern(context, binder)?;
        let named = matches!(
            context.storage[pattern].kind,
            PatternKind::Variable(_) | PatternKind::Named { .. }
        );
        if named {
            converted.push(pattern);
            continue;
        }
        let fallback = format_smolstr!("argument{position}");
        let type_id = context.checked.tree[binder].type_id;
        let name = context.type_parameter_name(type_id, fallback)?;
        let parameter = context.fresh_parameter(name)?;
        converted.push(context.pattern(PatternKind::Named { parameter, pattern }));
    }
    Ok(converted)
}

pub(super) fn guarded_expression(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    guarded: &checking_tree::GuardedExpression,
) -> ConversionResult<ExpressionId> {
    if let [alternative] = guarded.alternatives.as_ref()
        && alternative.pattern_guards.is_empty()
    {
        return where_expression(context, &alternative.where_expression);
    }

    let alternatives =
        guarded.alternatives.iter().map(|alternative| guarded_alternative(context, alternative));
    let alternatives = alternatives.collect::<ConversionResult<Vec<_>>>()?;
    Ok(context.expression(ExpressionKind::Guarded { alternatives: alternatives.into() }))
}

fn guarded_alternative(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    alternative: &checking_tree::GuardedAlternative,
) -> ConversionResult<GuardedAlternative> {
    let mut guards = Vec::new();
    for guard in alternative.pattern_guards.iter() {
        let guard = match guard {
            checking_tree::PatternGuard::Boolean { expression } => {
                Guard::Boolean(convert_expression(context, *expression)?)
            }
            checking_tree::PatternGuard::Pattern { binder, expression } => Guard::Pattern {
                expression: convert_expression(context, *expression)?,
                pattern: convert_pattern(context, *binder)?,
            },
        };
        guards.push(guard);
    }
    let expression = where_expression(context, &alternative.where_expression)?;
    Ok(GuardedAlternative { guards: guards.into(), expression })
}

fn where_expression(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    where_expression: &checking_tree::WhereExpression,
) -> ConversionResult<ExpressionId> {
    let expression = convert_expression(context, where_expression.expression)?;
    let_bindings(context, &where_expression.bindings, expression)
}

pub(super) fn let_bindings(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    bindings: &checking_tree::LetBindings,
    mut body: ExpressionId,
) -> ConversionResult<ExpressionId> {
    let checked = Arc::clone(&context.checked);
    for chunk in bindings.chunks.iter().rev() {
        match chunk {
            checking_tree::LetBindingChunk::Pattern { binder, where_expression: value, .. } => {
                let value = where_expression(context, value)?;
                let pattern = convert_pattern(context, *binder)?;
                body = context.expression(ExpressionKind::LetPattern { pattern, value, body });
            }
            checking_tree::LetBindingChunk::PatternError { source, .. } => {
                return Err(context.unsupported(UnsupportedState::PatternBindingError(*source)));
            }
            checking_tree::LetBindingChunk::Names { declarations, groups } => {
                let source_order = declarations
                    .iter()
                    .enumerate()
                    .map(|(position, &declaration)| (declaration, position));
                let source_order = source_order.collect::<FxHashMap<_, _>>();
                for group in groups.iter().rev() {
                    let mut converted = Vec::new();
                    for &source in group.as_slice() {
                        let Some(declaration_id) = checked.tree.lookup_let(source) else {
                            return Err(context
                                .unsupported(UnsupportedState::MissingLocalDeclaration(source)));
                        };
                        let declaration = &checked.tree[declaration_id];
                        let parameter = context.local_parameter(source)?;
                        let expression = value_declaration(context, &declaration.value)?;
                        let source_order = source_order[&declaration_id];
                        converted.push(Binding { parameter, expression, source_order });
                    }
                    body = context.expression(ExpressionKind::Let {
                        recursive: group.is_recursive(),
                        bindings: converted.into(),
                        body,
                    });
                }
            }
        }
    }
    Ok(body)
}
