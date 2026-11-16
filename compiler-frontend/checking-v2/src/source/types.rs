use std::sync::Arc;

use building_types::QueryResult;
use itertools::Itertools;
use lowering::{TypeKind, TypeVariableResolution};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use sugar::OperatorTree;

use crate::context::{CheckContext, Item};
use crate::core::{fold, kind, normalise, unify};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::state::{CheckState, TypeVariableKey};
use crate::types::{ForallBinder, RowField, SynonymApplication, Type, TypeId};
use crate::{CheckedSynonym, ExternalQueries};

#[derive(Clone, Copy)]
pub enum TypeArgument<'a> {
    Source(lowering::TypeId),
    Tree(&'a OperatorTree<lowering::TypeId>),
}

pub fn check_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::TypeId,
    expected_kind: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    state.with_crumb(ErrorCrumb::Type(id), |state| {
        let (t, k) = infer_type_core(state, context, id)?;
        instantiate_and_unify_kind(state, context, (t, k), expected_kind)
    })
}

pub fn infer_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    state.with_crumb(ErrorCrumb::Type(id), |state| infer_type_core(state, context, id))
}

fn instantiate_and_unify_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (t, k): (TypeId, TypeId),
    expected_kind: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let expected_head = normalise::normalise(state, context, expected_kind);
    let expects_forall = matches!(context.lookup_type(expected_head), Type::Forall(..));
    let (t, k) = if expects_forall { (t, k) } else { instantiate_kind(state, context, t, k) };
    unify::unify_kinds(state, context, expected_kind, k)?;
    Ok(t)
}

fn instantiate_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t: TypeId,
    k: TypeId,
) -> (TypeId, TypeId)
where
    Q: ExternalQueries,
{
    let mut t = t;
    let mut k = k;
    loop {
        let head = normalise::normalise(state, context, k);
        let Type::Forall(binder_id, inner_kind) = context.lookup_type(head) else {
            break (t, k);
        };
        let binder = context.lookup_forall_binder(binder_id);
        if binder.visible {
            break (t, k);
        }
        let argument = kind::fresh_unification(state, context, binder.kind);
        t = context.intern_kind_application(t, argument);
        k = fold::substitute_variable(context, binder.name, argument, inner_kind);
    }
}

fn unknown<Q>(context: &CheckContext<Q>) -> (TypeId, TypeId)
where
    Q: ExternalQueries,
{
    (context.types.unknown, context.types.unknown)
}

fn infer_type_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let Some(kind) = context.lowered.tree.get_type_kind(id) else {
        return Ok(unknown(context));
    };

    match kind {
        TypeKind::ApplicationChain { function, arguments } => {
            let Some(function) = *function else {
                return Ok(unknown(context));
            };
            let arguments = arguments.iter().map(|&argument| TypeArgument::Source(argument));
            let arguments = arguments.collect_vec();
            infer_application_chain(state, context, TypeArgument::Source(function), &arguments)
        }

        TypeKind::Arrow { argument, result } => {
            let argument = check_optional(state, context, *argument, context.types.type_)?;
            let result = check_optional(state, context, *result, context.types.type_)?;
            Ok((context.intern_function(argument, result), context.types.type_))
        }

        TypeKind::Constrained { constraint, constrained } => {
            let constraint = check_optional(state, context, *constraint, context.types.constraint)?;
            let constrained = check_optional(state, context, *constrained, context.types.type_)?;
            Ok((context.intern_constrained(constraint, constrained), context.types.type_))
        }

        TypeKind::Constructor { resolution } => {
            let Some(resolution) = *resolution else {
                return Ok(unknown(context));
            };
            infer_constructor(state, context, resolution, &[])
        }

        TypeKind::Forall { bindings, inner } => {
            let mut binders = Vec::new();
            for binding in bindings.iter() {
                let kind = match binding.kind {
                    Some(kind) => check_type(state, context, kind, context.types.type_)?,
                    None => kind::fresh_type(state, context),
                };
                let name = binding.name.clone();
                let text = name.unwrap_or_else(|| SmolStr::new_static("_"));
                let text = context.intern_text(text);
                let name = state.fresh_name();
                let binder = ForallBinder { visible: binding.visible, name, text, kind };
                let binder_id = context.intern_forall_binder(binder);
                let variable = context.intern_type(Type::Variable(binder_id));
                state.type_variables.insert(TypeVariableKey::Forall(binding.id), variable);
                binders.push(binder_id);
            }
            let inner = check_optional(state, context, *inner, context.types.type_)?;
            Ok((context.intern_forall_list(&binders, inner), context.types.type_))
        }

        TypeKind::Hole | TypeKind::Wildcard => {
            let kind = kind::fresh_type(state, context);
            let t = kind::fresh_unification(state, context, kind);
            Ok((t, kind))
        }

        TypeKind::Integer { value } => {
            let t = match value {
                Some(value) => context.intern_type(Type::Integer(*value)),
                None => context.types.unknown,
            };
            Ok((t, context.types.int))
        }

        TypeKind::String { value, .. } => {
            let t = match value {
                Some(value) => {
                    let text = String::from_utf16_lossy(value.as_utf16());
                    let text = context.intern_text(SmolStr::from(text));
                    context.intern_type(Type::String(text))
                }
                None => context.types.unknown,
            };
            Ok((t, context.types.symbol))
        }

        TypeKind::Kinded { type_, kind } => {
            let kind = check_optional(state, context, *kind, context.types.type_)?;
            let inner = check_optional(state, context, *type_, kind)?;
            Ok((context.intern_kinded(inner, kind), kind))
        }

        TypeKind::Operator { resolution } => {
            let Some(resolution) = *resolution else {
                return Ok(unknown(context));
            };
            let Some(target) = operator_target(context, resolution)? else {
                return Ok(unknown(context));
            };
            infer_constructor(state, context, target, &[])
        }

        TypeKind::OperatorChain { .. } => {
            let Some(Ok(tree)) = context.bracketed.types.get(&id) else {
                return Ok(unknown(context));
            };
            infer_operator_tree(state, context, tree)
        }

        TypeKind::Variable { resolution, .. } => {
            let Some(resolution) = *resolution else {
                return Ok(unknown(context));
            };
            let variable = lookup_type_variable(state, context, resolution);
            let kind = kind::kind_of(state, context, variable)?;
            Ok((variable, kind))
        }

        TypeKind::Record { items, tail } => {
            let row = infer_row(state, context, items, *tail, Some(context.types.type_))?;
            Ok((context.intern_record(row), context.types.type_))
        }

        TypeKind::Row { items, tail } => {
            let element = kind::fresh_type(state, context);
            let row = infer_row(state, context, items, *tail, Some(element))?;
            Ok((row, context.intern_row_kind(element)))
        }

        TypeKind::Parenthesized { parenthesized } => {
            let Some(parenthesized) = *parenthesized else {
                return Ok(unknown(context));
            };
            infer_type(state, context, parenthesized)
        }
    }
}

fn check_optional<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: Option<lowering::TypeId>,
    expected_kind: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    match id {
        Some(id) => check_type(state, context, id, expected_kind),
        None => Ok(context.types.unknown),
    }
}

fn infer_row<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[lowering::TypeRowItem],
    tail: Option<lowering::TypeId>,
    element: Option<TypeId>,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let element = match element {
        Some(element) => element,
        None => kind::fresh_type(state, context),
    };

    let mut fields = Vec::with_capacity(items.len());
    for item in items {
        let Some(label) = &item.name else {
            continue;
        };
        let id = check_optional(state, context, item.type_, element)?;
        fields.push(RowField { label: SmolStr::clone(label), id });
    }

    let row_kind = context.intern_row_kind(element);
    let tail = match tail {
        Some(tail) => Some(check_type(state, context, tail, row_kind)?),
        None => None,
    };

    Ok(context.intern_row(fields, tail))
}

pub fn lookup_type_variable<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: TypeVariableResolution,
) -> TypeId
where
    Q: ExternalQueries,
{
    let key = match resolution {
        TypeVariableResolution::Forall(binding_id) => TypeVariableKey::Forall(binding_id),
        TypeVariableResolution::Implicit(implicit) => {
            TypeVariableKey::Implicit(implicit.node, implicit.id)
        }
    };

    if let Some(&variable) = state.type_variables.get(&key) {
        return variable;
    }

    let TypeVariableResolution::Implicit(implicit) = resolution else {
        return context.types.unknown;
    };

    let text = implicit_variable_name(context, implicit);
    let text = context.intern_text(text);
    let kind = kind::fresh_type(state, context);
    let name = state.fresh_name();
    let binder = ForallBinder { visible: false, name, text, kind };
    let binder_id = context.intern_forall_binder(binder);
    let variable = context.intern_type(Type::Variable(binder_id));
    state.type_variables.insert(key, variable);
    state.implicit_binders.push(binder_id);
    variable
}

fn implicit_variable_name<Q>(
    context: &CheckContext<Q>,
    implicit: lowering::ImplicitTypeVariable,
) -> SmolStr
where
    Q: ExternalQueries,
{
    let node = &context.lowered.graph[implicit.node];
    let lowering::GraphNode::Implicit { bindings, .. } = node else {
        return SmolStr::new_static("_");
    };
    let Some((name, _)) = bindings.get_index(implicit.id) else {
        return SmolStr::new_static("_");
    };
    SmolStr::new(name)
}

pub fn operator_target<Q>(
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<Option<Item>>
where
    Q: ExternalQueries,
{
    let lowered = context.lowered_dependency(file_id)?;
    let Some(lowering::TypeItemKind::Operator { resolution, .. }) =
        lowered.tree.get_type_item_kind(type_id)
    else {
        return Ok(None);
    };
    Ok(*resolution)
}

fn infer_operator_tree<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    tree: &OperatorTree<lowering::TypeId>,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    match tree {
        OperatorTree::Leaf(None) => Ok(unknown(context)),
        OperatorTree::Leaf(Some(id)) => infer_type(state, context, *id),
        OperatorTree::Branch(operator_id, children) => {
            let Some(operator) = context.lowered.tree.get_type_operator(*operator_id) else {
                return Ok(unknown(context));
            };
            let Some(target) = operator_target(context, operator)? else {
                return Ok(unknown(context));
            };
            let [left, right] = &**children;
            let arguments = [TypeArgument::Tree(left), TypeArgument::Tree(right)];
            infer_constructor(state, context, target, &arguments)
        }
    }
}

fn infer_application_chain<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: TypeArgument<'_>,
    arguments: &[TypeArgument<'_>],
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    if let TypeArgument::Source(function) = function
        && let Some(resolution) = constructor_resolution(context, function)?
    {
        return state.with_crumb(ErrorCrumb::Type(function), |state| {
            infer_constructor(state, context, resolution, arguments)
        });
    }

    let mut function = infer_argument(state, context, function)?;
    for &argument in arguments {
        function = infer_application(state, context, function, argument)?;
    }
    Ok(function)
}

fn constructor_resolution<Q>(
    context: &CheckContext<Q>,
    id: lowering::TypeId,
) -> QueryResult<Option<Item>>
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_type_kind(id) {
        Some(TypeKind::Constructor { resolution }) => Ok(*resolution),
        Some(TypeKind::Operator { resolution: Some(resolution) }) => {
            operator_target(context, *resolution)
        }
        Some(TypeKind::Parenthesized { parenthesized: Some(inner) }) => {
            constructor_resolution(context, *inner)
        }
        _ => Ok(None),
    }
}

fn infer_argument<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: TypeArgument<'_>,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    match argument {
        TypeArgument::Source(id) => infer_type(state, context, id),
        TypeArgument::Tree(tree) => infer_operator_tree(state, context, tree),
    }
}

fn check_argument<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    argument: TypeArgument<'_>,
    expected_kind: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    match argument {
        TypeArgument::Source(id) => check_type(state, context, id, expected_kind),
        TypeArgument::Tree(tree) => {
            let inferred = infer_operator_tree(state, context, tree)?;
            instantiate_and_unify_kind(state, context, inferred, expected_kind)
        }
    }
}

pub fn infer_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (function, function_kind): (TypeId, TypeId),
    argument: TypeArgument<'_>,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let (function, function_kind) = instantiate_kind(state, context, function, function_kind);
    let function_kind = normalise::normalise(state, context, function_kind);

    if let Some((argument_kind, result_kind)) =
        normalise::function_parts(state, context, function_kind)
    {
        let argument = check_argument(state, context, argument, argument_kind)?;
        let application = context.intern_application(function, argument);
        return Ok((application, result_kind));
    }

    match context.lookup_type(function_kind) {
        Type::Unification(_) => {
            let argument_kind = kind::fresh_type(state, context);
            let result_kind = kind::fresh_type(state, context);
            let expected_kind = context.intern_function(argument_kind, result_kind);
            unify::unify_kinds(state, context, function_kind, expected_kind)?;
            let argument = check_argument(state, context, argument, argument_kind)?;
            let application = context.intern_application(function, argument);
            Ok((application, result_kind))
        }
        Type::Unknown => {
            infer_argument(state, context, argument)?;
            Ok(unknown(context))
        }
        _ => {
            state.insert_error(ErrorKind::CannotApplyType { function_kind });
            infer_argument(state, context, argument)?;
            Ok(unknown(context))
        }
    }
}

pub fn lookup_synonym<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<Option<CheckedSynonym>>
where
    Q: ExternalQueries,
{
    if file_id == context.id {
        if let Some(synonym) = state.pending_synonyms.get(&type_id) {
            return Ok(Some(CheckedSynonym::clone(synonym)));
        }
        return Ok(state.checked.synonyms.get(&type_id).cloned());
    }
    let checked = context.checked_dependency(file_id)?;
    Ok(checked.synonyms.get(&type_id).cloned())
}

fn infer_constructor<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: Item,
    arguments: &[TypeArgument<'_>],
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    if let Some(synonym) = lookup_synonym(state, context, resolution)? {
        return infer_synonym_application(state, context, resolution, synonym, arguments);
    }

    let constructor = context.intern_constructor(resolution);
    let kind = kind::lookup_type_item_kind(state, context, resolution)?;

    let mut function = (constructor, kind);
    for &argument in arguments {
        function = infer_application(state, context, function, argument)?;
    }
    Ok(function)
}

fn infer_synonym_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
    synonym: CheckedSynonym,
    arguments: &[TypeArgument<'_>],
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let Some(applied) = arguments.get(..synonym.parameters.len()) else {
        state.insert_error(ErrorKind::PartiallyAppliedSynonym);
        for &argument in arguments {
            infer_argument(state, context, argument)?;
        }
        return Ok(unknown(context));
    };
    let remaining = arguments.get(synonym.parameters.len()..).unwrap_or_default();

    let mut substitution = FxHashMap::default();
    for &binder_id in synonym.kind_binders.iter() {
        let binder = context.lookup_forall_binder(binder_id);
        let kind = fold::substitute_variables(context, &substitution, binder.kind);
        let argument = kind::fresh_unification(state, context, kind);
        substitution.insert(binder.name, argument);
    }

    let mut checked_arguments = Vec::with_capacity(applied.len());
    for (&binder_id, &argument) in synonym.parameters.iter().zip(applied) {
        let binder = context.lookup_forall_binder(binder_id);
        let kind = fold::substitute_variables(context, &substitution, binder.kind);
        let argument = check_argument(state, context, argument, kind)?;
        substitution.insert(binder.name, argument);
        checked_arguments.push(argument);
    }

    let expansion = fold::substitute_variables(context, &substitution, synonym.body);
    let arguments = Arc::from(checked_arguments);
    let application = SynonymApplication { file_id, type_id, arguments, expansion };
    let application = context.intern_synonym_application(application);
    let synonym_type = context.intern_type(Type::Synonym(application));
    let synonym_kind = kind::kind_of(state, context, expansion)?;

    let mut function = (synonym_type, synonym_kind);
    for &argument in remaining {
        function = infer_application(state, context, function, argument)?;
    }
    Ok(function)
}
