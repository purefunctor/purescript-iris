use std::sync::Arc;

use building_types::QueryResult;
use lowering::{BinderKind, BinderRecordItem, TypeKind};
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use sugar::OperatorTree;

use crate::ExternalQueries;
use crate::context::{CheckContext, TermItem};
use crate::core::{kind, normalise, skolem, unify};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::{terms, types};
use crate::state::{CheckState, LocalVariable, TermVariableKey};
use crate::tree::{self, Literal, TermReference};
use crate::types::{RowField, TypeId};

fn allocate(
    state: &mut CheckState,
    source: Option<lowering::BinderId>,
    type_id: TypeId,
    kind: tree::BinderKind,
) -> tree::BinderId {
    state.checked.tree.allocate_binder(source, type_id, kind)
}

fn bind_variable(
    state: &mut CheckState,
    key: TermVariableKey,
    binder: tree::BinderId,
    type_id: TypeId,
) {
    let reference = TermReference::Binder(binder);
    state.term_variables.insert(key, LocalVariable { type_id, reference });
}

pub fn binder_requires_monotype<Q>(context: &CheckContext<Q>, id: lowering::BinderId) -> bool
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_binder_kind(id) {
        Some(BinderKind::Wildcard | BinderKind::Variable { .. }) => false,
        Some(BinderKind::Named { binder: Some(inner), .. }) => {
            binder_requires_monotype(context, *inner)
        }
        Some(BinderKind::Parenthesized { parenthesized: Some(inner) }) => {
            binder_requires_monotype(context, *inner)
        }
        Some(BinderKind::Typed { binder, type_ }) => {
            type_.is_some_and(|type_| is_monotype(context, type_))
                || binder.is_some_and(|inner| binder_requires_monotype(context, inner))
        }
        _ => true,
    }
}

fn is_monotype<Q>(context: &CheckContext<Q>, id: lowering::TypeId) -> bool
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_type_kind(id) {
        Some(TypeKind::Forall { .. }) => false,
        Some(TypeKind::Parenthesized { parenthesized: Some(inner) }) => {
            is_monotype(context, *inner)
        }
        Some(TypeKind::Kinded { type_: Some(inner), .. }) => is_monotype(context, *inner),
        _ => true,
    }
}

pub fn binder_names<Q>(context: &CheckContext<Q>, id: lowering::BinderId, names: &mut Vec<SmolStr>)
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_binder_kind(id) {
        Some(BinderKind::Variable { variable: Some(name) }) => names.push(SmolStr::clone(name)),
        Some(BinderKind::Named { named, binder }) => {
            if let Some(name) = named {
                names.push(SmolStr::clone(name));
            }
            if let Some(binder) = binder {
                binder_names(context, *binder, names);
            }
        }
        Some(BinderKind::Typed { binder: Some(binder), .. })
        | Some(BinderKind::Parenthesized { parenthesized: Some(binder) }) => {
            binder_names(context, *binder, names)
        }
        Some(BinderKind::Constructor { arguments, .. })
        | Some(BinderKind::Array { array: arguments }) => {
            for &argument in arguments.iter() {
                binder_names(context, argument, names);
            }
        }
        Some(BinderKind::OperatorChain { head, tail }) => {
            if let Some(head) = head {
                binder_names(context, *head, names);
            }
            for pair in tail.iter() {
                if let Some(element) = pair.element {
                    binder_names(context, element, names);
                }
            }
        }
        Some(BinderKind::Record { record }) => {
            for item in record.iter() {
                match item {
                    BinderRecordItem::RecordField { value: Some(value), .. } => {
                        binder_names(context, *value, names)
                    }
                    BinderRecordItem::RecordPun { name: Some(name), .. } => {
                        names.push(SmolStr::clone(name))
                    }
                    _ => (),
                }
            }
        }
        _ => (),
    }
}

pub fn check_overlapping_names<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binders: &[lowering::BinderId],
) where
    Q: ExternalQueries,
{
    let mut names = Vec::new();
    for &binder in binders {
        binder_names(context, binder, &mut names);
    }
    let mut seen = FxHashSet::default();
    for name in names {
        if !seen.insert(SmolStr::clone(&name)) {
            state.insert_error(ErrorKind::OverlappingArgNames { name });
        }
    }
}

pub fn check_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::BinderId,
    expected: TypeId,
) -> QueryResult<tree::BinderId>
where
    Q: ExternalQueries,
{
    state
        .with_crumb(ErrorCrumb::Binder(id), |state| check_binder_core(state, context, id, expected))
}

fn literal_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::BinderId,
    expected: TypeId,
    literal_type: TypeId,
    literal: Option<Literal>,
) -> QueryResult<tree::BinderId>
where
    Q: ExternalQueries,
{
    unify::unify(state, context, expected, literal_type)?;
    let kind = match literal {
        Some(literal) => tree::BinderKind::Literal(literal),
        None => tree::BinderKind::Error,
    };
    Ok(allocate(state, Some(id), expected, kind))
}

fn check_binder_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::BinderId,
    expected: TypeId,
) -> QueryResult<tree::BinderId>
where
    Q: ExternalQueries,
{
    let Some(kind) = context.lowered.tree.get_binder_kind(id) else {
        return Ok(allocate(state, Some(id), expected, tree::BinderKind::Error));
    };

    match kind {
        BinderKind::Typed { binder, type_ } => {
            let annotation = match type_ {
                Some(type_) => types::check_type(state, context, *type_, context.types.type_)?,
                None => context.types.unknown,
            };
            unify::unify(state, context, expected, annotation)?;
            match binder {
                Some(binder) => check_binder(state, context, *binder, annotation),
                None => Ok(allocate(state, Some(id), annotation, tree::BinderKind::Error)),
            }
        }

        BinderKind::OperatorChain { .. } => {
            let Some(Ok(tree)) = context.bracketed.binders.get(&id) else {
                return Ok(allocate(state, Some(id), expected, tree::BinderKind::Error));
            };
            check_operator_tree(state, context, Some(id), tree, expected)
        }

        BinderKind::Integer { value } => {
            let literal = value.map(|value| Literal::Integer(i64::from(value)));
            literal_binder(state, context, id, expected, context.types.int, literal)
        }

        BinderKind::Number { negative, value } => {
            let literal = value.as_ref().map(|value| {
                let value = if *negative {
                    smol_str::format_smolstr!("-{value}")
                } else {
                    SmolStr::clone(value)
                };
                Literal::Number(value)
            });
            literal_binder(state, context, id, expected, context.types.number, literal)
        }

        BinderKind::String { value, .. } => {
            let literal = value.as_ref().map(|value| {
                Literal::String(SmolStr::from(String::from_utf16_lossy(value.as_utf16())))
            });
            literal_binder(state, context, id, expected, context.types.string, literal)
        }

        BinderKind::Char { value } => literal_binder(
            state,
            context,
            id,
            expected,
            context.types.char,
            value.map(Literal::Char),
        ),

        BinderKind::Boolean { boolean } => {
            let literal = Some(Literal::Boolean(*boolean));
            literal_binder(state, context, id, expected, context.types.boolean, literal)
        }

        BinderKind::Constructor { resolution, arguments } => {
            let Some(resolution) = *resolution else {
                for &argument in arguments.iter() {
                    let fresh = kind::fresh_type(state, context);
                    check_binder(state, context, argument, fresh)?;
                }
                return Ok(allocate(state, Some(id), expected, tree::BinderKind::Error));
            };
            let arguments = arguments.iter().map(|&argument| BinderArgument::Source(argument));
            let arguments = arguments.collect::<Vec<_>>();
            check_constructor(state, context, Some(id), resolution, &arguments, expected)
        }

        BinderKind::Variable { variable } => {
            let name = variable.clone().unwrap_or_else(|| SmolStr::new_static("_"));
            let binder = allocate(state, Some(id), expected, tree::BinderKind::Variable(name));
            bind_variable(state, TermVariableKey::Binder(id), binder, expected);
            Ok(binder)
        }

        BinderKind::Named { named, binder } => {
            let inner = match binder {
                Some(binder) => check_binder(state, context, *binder, expected)?,
                None => allocate(state, None, expected, tree::BinderKind::Error),
            };
            let name = named.clone().unwrap_or_else(|| SmolStr::new_static("_"));
            let named = allocate(state, Some(id), expected, tree::BinderKind::Named(name, inner));
            bind_variable(state, TermVariableKey::Binder(id), named, expected);
            Ok(named)
        }

        BinderKind::Wildcard => Ok(allocate(state, Some(id), expected, tree::BinderKind::Wildcard)),

        BinderKind::Array { array } => {
            let element = kind::fresh_type(state, context);
            let array_type = context.intern_application(context.types.array, element);
            unify::unify(state, context, expected, array_type)?;
            let elements =
                array.iter().map(|&binder| check_binder(state, context, binder, element));
            let elements = elements.collect::<QueryResult<Vec<_>>>()?;
            let kind = tree::BinderKind::Array(Arc::from(elements));
            Ok(allocate(state, Some(id), expected, kind))
        }

        BinderKind::Record { record } => {
            let mut seen = FxHashSet::default();
            let mut fields = Vec::with_capacity(record.len());
            let mut pending = Vec::with_capacity(record.len());
            for item in record.iter() {
                let label = match item {
                    BinderRecordItem::RecordField { name: Some(name), .. }
                    | BinderRecordItem::RecordPun { name: Some(name), .. } => SmolStr::clone(name),
                    _ => continue,
                };
                if !seen.insert(SmolStr::clone(&label)) {
                    state.insert_error(ErrorKind::DuplicateLabel { label });
                    continue;
                }
                let field = kind::fresh_type(state, context);
                fields.push(RowField { label: SmolStr::clone(&label), id: field });
                pending.push((label, field, item));
            }

            let rest = kind::fresh_unification(state, context, context.types.row_type);
            let row = context.intern_row(fields, Some(rest));
            let record_type = context.intern_record(row);
            unify::unify(state, context, expected, record_type)?;

            let mut checked = Vec::with_capacity(pending.len());
            for (label, field, item) in pending {
                let binder = match item {
                    BinderRecordItem::RecordField { value: Some(value), .. } => {
                        check_binder(state, context, *value, field)?
                    }
                    BinderRecordItem::RecordPun { id: pun, .. } => {
                        let kind = tree::BinderKind::Variable(SmolStr::clone(&label));
                        let binder = allocate(state, None, field, kind);
                        bind_variable(state, TermVariableKey::Pun(*pun), binder, field);
                        binder
                    }
                    BinderRecordItem::RecordField { value: None, .. } => {
                        allocate(state, None, field, tree::BinderKind::Error)
                    }
                };
                checked.push((label, binder));
            }

            let kind = tree::BinderKind::Record(Arc::from(checked));
            Ok(allocate(state, Some(id), expected, kind))
        }

        BinderKind::Parenthesized { parenthesized } => match parenthesized {
            Some(inner) => check_binder(state, context, *inner, expected),
            None => Ok(allocate(state, Some(id), expected, tree::BinderKind::Error)),
        },
    }
}

#[derive(Clone, Copy)]
enum BinderArgument<'a> {
    Source(lowering::BinderId),
    Tree(&'a OperatorTree<lowering::BinderId>),
}

fn check_binder_argument<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: BinderArgument<'_>,
    expected: TypeId,
) -> QueryResult<tree::BinderId>
where
    Q: ExternalQueries,
{
    match argument {
        BinderArgument::Source(id) => check_binder(state, context, id, expected),
        BinderArgument::Tree(tree) => check_operator_tree(state, context, None, tree, expected),
    }
}

fn check_operator_tree<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: Option<lowering::BinderId>,
    tree: &OperatorTree<lowering::BinderId>,
    expected: TypeId,
) -> QueryResult<tree::BinderId>
where
    Q: ExternalQueries,
{
    match tree {
        OperatorTree::Leaf(Some(id)) => check_binder(state, context, *id, expected),
        OperatorTree::Leaf(None) => Ok(allocate(state, source, expected, tree::BinderKind::Error)),
        OperatorTree::Branch(operator_id, children) => {
            let Some(operator) = context.lowered.tree.get_term_operator(*operator_id) else {
                return Ok(allocate(state, source, expected, tree::BinderKind::Error));
            };
            let Some(target) = terms::operator_target(context, operator)? else {
                return Ok(allocate(state, source, expected, tree::BinderKind::Error));
            };
            let [left, right] = &**children;
            let arguments = [BinderArgument::Tree(left), BinderArgument::Tree(right)];
            check_constructor(state, context, source, target, &arguments, expected)
        }
    }
}

fn check_constructor<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source: Option<lowering::BinderId>,
    (file_id, item_id): TermItem,
    arguments: &[BinderArgument<'_>],
    expected: TypeId,
) -> QueryResult<tree::BinderId>
where
    Q: ExternalQueries,
{
    let constructor_type = terms::lookup_term_type(state, context, (file_id, item_id))?;
    let instantiated = skolem::instantiate(state, context, constructor_type)?;

    let mut fields = Vec::new();
    let mut result = instantiated.inner;
    while let Some((argument, next)) = normalise::function_parts(state, context, result) {
        fields.push(argument);
        result = next;
    }

    if fields.len() != arguments.len() {
        let expected_arity = fields.len();
        let actual = arguments.len();
        state.insert_error(ErrorKind::IncorrectConstructorArity {
            expected: expected_arity,
            actual,
        });
        for &argument in arguments {
            let fresh = kind::fresh_type(state, context);
            check_binder_argument(state, context, argument, fresh)?;
        }
        return Ok(allocate(state, source, expected, tree::BinderKind::Error));
    }

    unify::unify(state, context, expected, result)?;

    let mut checked = Vec::with_capacity(arguments.len());
    for (&argument, &field) in arguments.iter().zip(&fields) {
        checked.push(check_binder_argument(state, context, argument, field)?);
    }

    let kind =
        tree::BinderKind::Constructor { file_id, item: item_id, arguments: Arc::from(checked) };
    Ok(allocate(state, source, expected, kind))
}
