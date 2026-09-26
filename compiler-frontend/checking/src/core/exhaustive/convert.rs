use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use itertools::Itertools;
use lowering::{BinderId, TermOperatorId};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use sugar::OperatorTree;

use crate::context::CheckContext;
use crate::core::exhaustive::{PatternConstructor, PatternId};
use crate::core::{Type, TypeId, normalise};
use crate::state::CheckState;
use crate::{ExternalQueries, OperatorBranchTypes, safe_loop};

pub fn convert_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: BinderId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    let t = state
        .checked
        .node_types
        .lookup_binder(id)
        .unwrap_or_else(|| context.unknown("unresolved binder"));

    let Some(kind) = context.lowered.tree.get_binder_kind(id) else {
        return Ok(state.allocate_wildcard(t));
    };

    match kind {
        lowering::BinderKind::Typed { binder, .. } => match binder {
            Some(id) => convert_binder(state, context, *id),
            None => Ok(state.allocate_wildcard(t)),
        },
        lowering::BinderKind::OperatorChain { .. } => {
            convert_operator_chain_binder(state, context, id, t)
        }
        lowering::BinderKind::Integer { value } => match value {
            Some(v) => {
                let constructor = PatternConstructor::Integer(*v);
                Ok(state.allocate_constructor(constructor, t))
            }
            None => Ok(state.allocate_wildcard(t)),
        },
        lowering::BinderKind::Number { negative, value } => {
            if let Some(value) = value {
                let constructor = PatternConstructor::Number(*negative, SmolStr::clone(value));
                Ok(state.allocate_constructor(constructor, t))
            } else {
                Ok(state.allocate_wildcard(t))
            }
        }
        lowering::BinderKind::Constructor { resolution, arguments } => {
            convert_constructor_binder(state, context, resolution, arguments, t)
        }
        lowering::BinderKind::Variable { .. } => Ok(state.allocate_wildcard(t)),
        lowering::BinderKind::Named { binder, .. } => match binder {
            Some(id) => convert_binder(state, context, *id),
            None => Ok(state.allocate_wildcard(t)),
        },
        lowering::BinderKind::Wildcard => Ok(state.allocate_wildcard(t)),
        lowering::BinderKind::String { value, .. } => {
            if let Some(value) = value {
                let constructor = PatternConstructor::String(value.clone());
                Ok(state.allocate_constructor(constructor, t))
            } else {
                Ok(state.allocate_wildcard(t))
            }
        }
        lowering::BinderKind::Char { value } => match value {
            Some(v) => {
                let constructor = PatternConstructor::Char(*v);
                Ok(state.allocate_constructor(constructor, t))
            }
            None => Ok(state.allocate_wildcard(t)),
        },
        lowering::BinderKind::Boolean { boolean } => {
            let constructor = PatternConstructor::Boolean(*boolean);
            Ok(state.allocate_constructor(constructor, t))
        }
        lowering::BinderKind::Array { array } => lower_array_binder(state, context, array, t),
        lowering::BinderKind::Record { record } => lower_record_binder(state, context, record, t),
        lowering::BinderKind::Parenthesized { parenthesized } => match parenthesized {
            Some(id) => convert_binder(state, context, *id),
            None => Ok(state.allocate_wildcard(t)),
        },
    }
}

fn lower_array_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    array: &[BinderId],
    t: TypeId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    let mut fields = Vec::new();
    for &element in array {
        fields.push(convert_binder(state, context, element)?);
    }
    let constructor = PatternConstructor::Array { fields };
    Ok(state.allocate_constructor(constructor, t))
}

fn lower_record_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: &[lowering::BinderRecordItem],
    t: TypeId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    match try_build_record_constructor(state, context, record, t)? {
        Some((labels, fields)) => {
            let constructor = PatternConstructor::Record { labels, fields };
            Ok(state.allocate_constructor(constructor, t))
        }
        None => {
            // Fallback: use a wildcard when we can't build a canonical record constructor
            Ok(state.allocate_wildcard(t))
        }
    }
}

fn try_build_record_constructor<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    record: &[lowering::BinderRecordItem],
    t: TypeId,
) -> QueryResult<Option<(Vec<SmolStr>, Vec<PatternId>)>>
where
    Q: ExternalQueries,
{
    let expanded_t = normalise_expand_type(state, context, t)?;

    let (constructor, arguments) = extract_type_application(state, context, expanded_t)?;

    if constructor != context.prim.record {
        return Ok(None);
    }

    let Some(row_type_id) = arguments.first() else {
        return Ok(None);
    };

    let row_type_id = normalise::expand(state, context, *row_type_id)?;

    let row_fields = if let Type::Row(row_type_id) = *context.lookup_type(row_type_id) {
        Arc::clone(&context.lookup_row_type(row_type_id).fields)
    } else {
        return Ok(None);
    };

    let field_type_map: FxHashMap<SmolStr, TypeId> =
        row_fields.iter().map(|field| (SmolStr::clone(&field.label), field.id)).collect();

    let mut provided_patterns = FxHashMap::default();
    for element in record.iter() {
        match element {
            lowering::BinderRecordItem::RecordField { name, value } => {
                let Some(name) = name.clone() else {
                    continue;
                };

                let pattern = if let Some(value) = value {
                    convert_binder(state, context, *value)?
                } else {
                    state.allocate_wildcard(context.unknown("record field"))
                };

                provided_patterns.insert(name, pattern);
            }
            lowering::BinderRecordItem::RecordPun { id: _, name } => {
                let Some(name) = name.clone() else {
                    continue;
                };

                let t = field_type_map
                    .get(&name)
                    .copied()
                    .unwrap_or_else(|| context.unknown("record pun"));

                let pattern = state.allocate_wildcard(t);
                provided_patterns.insert(name, pattern);
            }
        }
    }

    let mut sorted_labels = field_type_map.keys().cloned().collect_vec();
    sorted_labels.sort();

    let mut labels = Vec::with_capacity(sorted_labels.len());
    let mut fields = Vec::with_capacity(sorted_labels.len());

    for label in sorted_labels {
        let pattern = provided_patterns.get(&label).copied().unwrap_or_else(|| {
            let t = field_type_map[&label];
            state.allocate_wildcard(t)
        });
        labels.push(label);
        fields.push(pattern);
    }

    Ok(Some((labels, fields)))
}

fn convert_constructor_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: &Option<(FileId, TermItemId)>,
    arguments: &Arc<[BinderId]>,
    t: TypeId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    let Some((file_id, item_id)) = *resolution else {
        return Ok(state.allocate_wildcard(t));
    };

    let mut fields = Vec::new();
    for &argument in arguments.iter() {
        fields.push(convert_binder(state, context, argument)?);
    }

    let constructor = PatternConstructor::DataConstructor { file_id, item_id, fields };
    Ok(state.allocate_constructor(constructor, t))
}

fn convert_operator_chain_binder<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: BinderId,
    t: TypeId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    let Some(tree) = context.bracketed.binders.get(&id) else {
        return Ok(state.allocate_wildcard(t));
    };

    let Ok(tree) = tree else {
        return Ok(state.allocate_wildcard(t));
    };

    convert_operator_tree(state, context, tree, t)
}

fn convert_operator_tree<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    tree: &OperatorTree<BinderId>,
    t: TypeId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    match tree {
        OperatorTree::Leaf(None) => Ok(state.allocate_wildcard(t)),
        OperatorTree::Leaf(Some(binder_id)) => convert_binder(state, context, *binder_id),
        OperatorTree::Branch(operator_id, children) => {
            convert_operator_branch(state, context, *operator_id, children, t)
        }
    }
}

fn convert_operator_branch<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    operator_id: TermOperatorId,
    children: &[OperatorTree<BinderId>; 2],
    t: TypeId,
) -> QueryResult<PatternId>
where
    Q: ExternalQueries,
{
    let Some((file_id, item_id)) = context.lowered.tree.get_term_operator(operator_id) else {
        return Ok(state.allocate_wildcard(t));
    };

    // The operator_id points to itself, thus we need to follow the
    // resolution to find the constructor that it actually points to.
    let Some((constructor_file_id, constructor_item_id)) =
        resolve_term_operator(context, file_id, item_id)?
    else {
        return Ok(state.allocate_wildcard(t));
    };

    let Some(OperatorBranchTypes { left, right, result }) =
        state.checked.node_types.lookup_term_operator(operator_id)
    else {
        return Ok(state.allocate_wildcard(t));
    };

    let [left_tree, right_tree] = children;

    let left_pattern = convert_operator_tree(state, context, left_tree, left)?;
    let right_pattern = convert_operator_tree(state, context, right_tree, right)?;

    let constructor = PatternConstructor::DataConstructor {
        file_id: constructor_file_id,
        item_id: constructor_item_id,
        fields: vec![left_pattern, right_pattern],
    };

    Ok(state.allocate_constructor(constructor, result))
}

fn resolve_term_operator<Q>(
    context: &CheckContext<Q>,
    file_id: FileId,
    item_id: TermItemId,
) -> QueryResult<Option<(FileId, TermItemId)>>
where
    Q: ExternalQueries,
{
    let on_lowered = |lowered: &lowering::LoweredModule| {
        if let Some(lowering::TermItemKind::Operator { resolution, .. }) =
            lowered.tree.get_term_item_kind(item_id)
        {
            *resolution
        } else {
            None
        }
    };

    if file_id == context.id {
        Ok(on_lowered(&context.lowered))
    } else {
        let lowered = context.queries.lowered(file_id)?;
        Ok(on_lowered(&lowered))
    }
}

fn extract_type_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut type_id: TypeId,
) -> QueryResult<(TypeId, Vec<TypeId>)>
where
    Q: ExternalQueries,
{
    let mut arguments = Vec::new();

    safe_loop! {
        type_id = normalise::expand(state, context, type_id)?;
        match *context.lookup_type(type_id) {
            Type::Application(function, argument) => {
                arguments.push(argument);
                type_id = function;
            }
            Type::KindApplication(function, _) => {
                type_id = function;
            }
            _ => break,
        }
    }

    arguments.reverse();
    Ok((type_id, arguments))
}

fn normalise_expand_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    normalise::expand(state, context, type_id)
}
