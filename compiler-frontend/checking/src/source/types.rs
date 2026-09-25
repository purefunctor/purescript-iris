//! Implements syntax-driven checking rules for types.

pub mod application;

use std::sync::Arc;

use building_types::QueryResult;
use itertools::Itertools;
use lowering::GraphNode;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;

use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{ForallBinder, RowField, Type, TypeId, normalise, toolkit, unification};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::holes::{HoleBinding, TypeHole};
use crate::source::{operator, synonym};
use crate::state::{CheckState, SourceTypeVariableKey};
use crate::{ExternalQueries, safe_loop};

const MISSING_NAME: SmolStr = SmolStr::new_static("<MissingName>");

/// Checks the kind of a syntax type against a core type.
///
/// This function returns the core type and kind.
pub fn check_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source_type: lowering::TypeId,
    expected_kind: TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    state.with_error_crumb(ErrorCrumb::CheckingKind(source_type), |state| {
        let (inferred_type, inferred_kind) =
            check_kind_core(state, context, source_type, expected_kind)?;
        state.checked.node_types.type_kinds.insert(source_type, inferred_kind);
        Ok((inferred_type, inferred_kind))
    })
}

fn check_kind_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source_type: lowering::TypeId,
    expected_kind: TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let (inferred_type, inferred_kind) = infer_kind(state, context, source_type)?;
    let (inferred_type, inferred_kind) =
        instantiate_kind_applications(state, context, inferred_type, inferred_kind, expected_kind)?;

    unification::subtype(state, context, inferred_kind, expected_kind)?;
    Ok((inferred_type, inferred_kind))
}

pub fn infer_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    source_type: lowering::TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    state.with_error_crumb(ErrorCrumb::InferringKind(source_type), |state| {
        let (inferred_type, inferred_kind) = infer_kind_core(state, context, source_type)?;
        state.checked.node_types.type_kinds.insert(source_type, inferred_kind);
        Ok((inferred_type, inferred_kind))
    })
}

fn infer_kind_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let unknown = |message: &str| {
        let u = context.unknown(message);
        (u, u)
    };

    let Some(kind) = context.lowered.tree.get_type_kind(id) else {
        return Ok(unknown("missing syntax"));
    };

    match kind {
        lowering::TypeKind::ApplicationChain { function, arguments } => {
            let Some(function) = function else {
                return Ok(unknown("missing application function"));
            };

            if let Some(synonym) = synonym::parse_synonym(state, context, *function)? {
                return synonym::infer_synonym_application(state, context, id, synonym, arguments);
            }

            let (mut t, mut k) = infer_kind(state, context, *function)?;

            for argument in arguments.iter() {
                (t, k) = infer_application_kind(state, context, (t, k), *argument)?;
            }

            Ok((t, k))
        }

        lowering::TypeKind::Arrow { argument, result } => {
            let argument = if let Some(argument) = argument {
                let (argument, _) = check_kind(state, context, *argument, context.prim.t)?;
                argument
            } else {
                context.unknown("missing function argument")
            };

            let result = if let Some(result) = result {
                let (result, _) = check_kind(state, context, *result, context.prim.t)?;
                result
            } else {
                context.unknown("missing function result")
            };

            let t = context.intern_function(argument, result);
            let k = context.prim.t;

            Ok((t, k))
        }

        lowering::TypeKind::Constrained { constraint, constrained } => {
            let constraint = if let Some(constraint) = constraint {
                let (constraint, _) =
                    check_kind(state, context, *constraint, context.prim.constraint)?;
                constraint
            } else {
                context.unknown("missing constraint")
            };

            let constrained = if let Some(constrained) = constrained {
                let (constrained, _) = infer_kind(state, context, *constrained)?;
                constrained
            } else {
                context.unknown("missing constrained")
            };

            let t = context.intern_constrained(constraint, constrained);
            let k = context.prim.t;

            Ok((t, k))
        }

        lowering::TypeKind::Constructor { resolution } => {
            let Some((file_id, type_id)) = *resolution else {
                return Ok(unknown("missing constructor"));
            };

            if let Some(synonym) = synonym::parse_synonym(state, context, id)? {
                return synonym::infer_synonym_constructor(state, context, synonym, id);
            }

            let t = context.queries.intern_type(Type::Constructor(file_id, type_id));
            let k = toolkit::lookup_file_type(state, context, file_id, type_id)?;

            Ok((t, k))
        }

        lowering::TypeKind::Forall { bindings, inner } => {
            let binders = bindings
                .iter()
                .map(|binding| check_type_variable_binding(state, context, binding))
                .collect::<QueryResult<Vec<_>>>()?;

            let inner = if let Some(inner) = inner {
                let (inner, _) = check_kind(state, context, *inner, context.prim.t)?;
                inner
            } else {
                context.unknown("missing forall inner")
            };

            let t = binders.iter().rfold(inner, |inner, binder| {
                let binder_id = context.intern_forall_binder(*binder);
                context.intern_forall(binder_id, inner)
            });

            Ok((t, context.prim.t))
        }

        lowering::TypeKind::Hole => {
            let kind_id = state.fresh_unification(context.queries, context.prim.t);
            let type_id = state.fresh_unification(context.queries, kind_id);

            let bindings = type_hole_bindings(state, context, id);
            state.checked.holes.types.insert(id, TypeHole { type_id, kind_id, bindings });
            state.insert_error(ErrorKind::TypeHole { source_type: id });

            Ok((type_id, kind_id))
        }

        lowering::TypeKind::Integer { value } => {
            let t = if let Some(value) = value {
                context.queries.intern_type(Type::Integer(*value))
            } else {
                context.unknown("missing integer value")
            };
            Ok((t, context.prim.int))
        }

        lowering::TypeKind::Kinded { type_, kind } => {
            let k = if let Some(kind) = kind {
                let (k, _) = infer_kind(state, context, *kind)?;
                k
            } else {
                context.unknown("missing kinded kind")
            };
            let t = if let Some(type_) = type_ {
                let (t, _) = check_kind(state, context, *type_, k)?;
                t
            } else {
                context.unknown("missing kinded type")
            };
            Ok((t, k))
        }

        lowering::TypeKind::Operator { resolution } => {
            let Some((file_id, type_id)) = *resolution else {
                return Ok(unknown("missing operator"));
            };

            let Some((file_id, type_id)) =
                toolkit::resolve_type_operator_target(context, file_id, type_id)?
            else {
                return Ok(unknown("missing operator"));
            };

            let t = context.queries.intern_type(Type::Constructor(file_id, type_id));
            let k = toolkit::lookup_file_type(state, context, file_id, type_id)?;

            Ok((t, k))
        }

        lowering::TypeKind::OperatorChain { .. } => {
            operator::infer_operator_chain(state, context, id)
        }

        lowering::TypeKind::String { kind, value } => {
            let value = value.clone().unwrap_or_else(|| MISSING_NAME.as_str().into());

            let t = context.queries.intern_type(Type::String(*kind, value));
            let k = context.prim.symbol;

            Ok((t, k))
        }

        lowering::TypeKind::Variable { name, resolution } => match resolution {
            Some(lowering::TypeVariableResolution::Forall(forall)) => {
                let variable = state
                    .lookup_source_type_variable(context, SourceTypeVariableKey::Forall(*forall))?
                    .expect("invariant violated: KindScope::bind_forall");

                let t = context.intern_rigid(variable.name, variable.depth, variable.kind);

                Ok((t, variable.kind))
            }

            Some(lowering::TypeVariableResolution::Implicit(implicit)) => {
                let key = SourceTypeVariableKey::Implicit { node: implicit.node, id: implicit.id };
                if implicit.binding {
                    let n = state.names.fresh();
                    let k = state.fresh_unification(context.queries, context.prim.t);

                    if let Some(text) = name {
                        let text = SmolStr::clone(text);
                        let text = context.queries.intern_smol_str(text);
                        state.checked.names.insert(n, text);
                    }

                    state.bindings.bind_implicit(implicit.node, implicit.id, n, state.depth, k);
                    state
                        .checked
                        .node_types
                        .implicit_bindings
                        .insert((implicit.node, implicit.id), k);

                    let t = context.intern_rigid(n, state.depth, k);

                    Ok((t, k))
                } else {
                    let variable = state
                        .lookup_source_type_variable(context, key)?
                        .expect("invariant violated: KindScope::bind_implicit");

                    let t = context.intern_rigid(variable.name, variable.depth, variable.kind);

                    Ok((t, variable.kind))
                }
            }

            None => {
                let name = name.clone().unwrap_or(MISSING_NAME);
                let id = context.queries.intern_smol_str(name);

                let t = context.queries.intern_type(Type::Free(id));
                let k = state.fresh_unification(context.queries, context.prim.t);

                Ok((t, k))
            }
        },

        lowering::TypeKind::Wildcard => {
            let k = state.fresh_unification(context.queries, context.prim.t);
            let t = state.fresh_unification(context.queries, k);
            Ok((t, k))
        }

        lowering::TypeKind::Record { items, tail } => {
            let (row_type, row_kind) =
                infer_row_kind(state, context, items, tail, EmptyRowTail::Check)?;
            unification::subtype(state, context, row_kind, context.prim.row_type)?;

            let t = context.intern_application(context.prim.record, row_type);
            let k = context.prim.t;

            Ok((t, k))
        }

        lowering::TypeKind::Row { items, tail } => {
            infer_row_kind(state, context, items, tail, EmptyRowTail::Infer)
        }

        lowering::TypeKind::Parenthesized { parenthesized } => {
            if let Some(parenthesized) = parenthesized {
                infer_kind(state, context, *parenthesized)
            } else {
                Ok(unknown("missing parenthesized"))
            }
        }
    }
}

fn type_hole_bindings<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    source_type: lowering::TypeId,
) -> Arc<[HoleBinding]>
where
    Q: ExternalQueries,
{
    let mut seen: FxHashSet<&SmolStr> = FxHashSet::default();
    let mut result = vec![];

    if let Some(scope_node) = context.lowered.nodes.type_node(source_type) {
        collect_graph_type_bindings(state, context, scope_node, &mut seen, &mut result);
    }

    Arc::from(result)
}

fn collect_graph_type_bindings<'a, Q>(
    state: &CheckState,
    context: &'a CheckContext<Q>,
    scope_node: lowering::GraphNodeId,
    seen: &mut FxHashSet<&'a SmolStr>,
    result: &mut Vec<HoleBinding>,
) where
    Q: ExternalQueries,
{
    for (node_id, node) in context.lowered.graph.traverse(scope_node) {
        match node {
            GraphNode::Forall { bindings, .. } => {
                let mut bindings = bindings.iter().collect_vec();
                bindings.sort_by_key(|(name, _)| *name);

                for (name, binding_id) in bindings {
                    collect_forall_binding(state, name, *binding_id, seen, result);
                }
            }
            GraphNode::Implicit { bindings, .. } => {
                let mut entries = bindings.iter_indexed().collect_vec();
                entries.sort_by_key(|(name, _)| *name);

                for (name, binding_id) in entries {
                    collect_implicit_binding(state, node_id, name, binding_id, seen, result);
                }
            }
            GraphNode::Binder { .. } | GraphNode::Let { .. } => {}
        }
    }
}

fn collect_forall_binding<'a>(
    state: &CheckState,
    name: &'a SmolStr,
    binding_id: lowering::TypeVariableBindingId,
    seen: &mut FxHashSet<&'a SmolStr>,
    result: &mut Vec<HoleBinding>,
) {
    let Some(type_id) = state.checked.node_types.lookup_forall_binding(binding_id).or_else(|| {
        let key = SourceTypeVariableKey::Forall(binding_id);
        state.bindings.lookup(key).map(|variable| variable.kind)
    }) else {
        return;
    };

    if seen.insert(name) {
        result.push(HoleBinding { name: SmolStr::clone(name), type_id });
    }
}

fn collect_implicit_binding<'a>(
    state: &CheckState,
    node_id: lowering::GraphNodeId,
    name: &'a SmolStr,
    binding_id: lowering::ImplicitBindingId,
    seen: &mut FxHashSet<&'a SmolStr>,
    result: &mut Vec<HoleBinding>,
) {
    let Some(type_id) =
        state.checked.node_types.lookup_implicit_binding(node_id, binding_id).or_else(|| {
            let key = SourceTypeVariableKey::Implicit { node: node_id, id: binding_id };
            state.bindings.lookup(key).map(|variable| variable.kind)
        })
    else {
        return;
    };

    if seen.insert(name) {
        result.push(HoleBinding { name: SmolStr::clone(name), type_id });
    }
}

/// Instantiates kind-level foralls using [`Type::KindApplication`].
///
/// If the inferred kind is polymorphic and the expected kind is monomorphic,
/// this function adds the necessary kind applications to the inferred type.
fn instantiate_kind_applications<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut t: TypeId,
    mut k: TypeId,
    expected_kind: TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let expected_kind = normalise::expand(state, context, expected_kind)?;

    if matches!(*context.lookup_type(expected_kind), Type::Forall(_, _)) {
        return Ok((t, k));
    }

    safe_loop! {
        k = normalise::expand(state, context, k)?;

        let Type::Forall(binder_id, inner_kind) = *context.lookup_type(k) else {
            break;
        };

        let binder = context.lookup_forall_binder(binder_id);
        let binder_kind = normalise::expand(state, context, binder.kind)?;

        let argument_type = state.fresh_unification(context.queries, binder_kind);
        t = context.intern_kind_application(t, argument_type);
        k = SubstituteName::one(state, context, binder.name, argument_type, inner_kind)?;
    }

    Ok((t, k))
}

fn check_type_variable_binding<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    binding: &lowering::TypeVariableBinding,
) -> QueryResult<ForallBinder>
where
    Q: ExternalQueries,
{
    let kind = if let Some(kind_id) = binding.kind {
        let (kind, _) = check_kind(state, context, kind_id, context.prim.t)?;
        kind
    } else {
        state.fresh_unification(context.queries, context.prim.t)
    };

    let visible = binding.visible;
    let name = state.names.fresh();

    let text = if let Some(name) = &binding.name { SmolStr::clone(name) } else { name.as_text() };
    let text = context.queries.intern_smol_str(text);

    state.checked.names.insert(name, text);
    state.checked.node_types.forall_bindings.insert(binding.id, kind);
    state.bindings.bind_forall(binding.id, name, state.depth, kind);
    Ok(ForallBinder { visible, name, kind, scope: None })
}

pub fn infer_application_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (function_type, function_kind): (TypeId, TypeId),
    argument: lowering::TypeId,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let ((result_type, result_kind), _) = application::infer_application_kind(
        state,
        context,
        (function_type, function_kind),
        application::Argument::Syntax(argument),
        application::Options::TYPES,
        application::Records::Ignore,
    )?;

    Ok((result_type, result_kind))
}

#[derive(Clone, Copy)]
enum EmptyRowTail {
    Check,
    Infer,
}

fn infer_row_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &Arc<[lowering::TypeRowItem]>,
    tail: &Option<lowering::TypeId>,
    empty_tail: EmptyRowTail,
) -> QueryResult<(TypeId, TypeId)>
where
    Q: ExternalQueries,
{
    let field_kind = state.fresh_unification(context.queries, context.prim.t);
    let row_kind = context.intern_application(context.prim.row, field_kind);

    if items.is_empty()
        && let Some(tail) = tail
    {
        return match empty_tail {
            EmptyRowTail::Check => check_kind(state, context, *tail, row_kind),
            EmptyRowTail::Infer => infer_kind(state, context, *tail),
        };
    }

    let fields = items.iter().map(|item| {
        let label = item.name.clone().unwrap_or(MISSING_NAME);
        let id = if let Some(t) = item.type_ {
            let (t, _) = check_kind(state, context, t, field_kind)?;
            t
        } else {
            context.unknown("missing field type")
        };

        Ok(RowField { label, id })
    });

    let fields = fields.collect::<QueryResult<Vec<_>>>()?;

    let tail = if let Some(tail) = tail {
        let (tail_type, _) = check_kind(state, context, *tail, row_kind)?;
        Some(tail_type)
    } else {
        None
    };

    let row_type = context.intern_row(fields, tail);

    Ok((row_type, row_kind))
}

pub fn elaborate_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let unknown = || context.unknown("invalid kind");
    let id = normalise::expand(state, context, id)?;

    let kind = match *context.lookup_type(id) {
        Type::Application(function, _) => {
            let function_kind = elaborate_kind(state, context, function)?;
            let function_kind = normalise::expand(state, context, function_kind)?;

            match *context.lookup_type(function_kind) {
                Type::Function(_, result_kind) => result_kind,

                Type::Unification(unification_id) => {
                    let depth = state.unifications.get(unification_id).depth;

                    let argument_u = state.unifications.fresh(depth, context.prim.t);
                    let argument_u = context.queries.intern_type(Type::Unification(argument_u));

                    let result_u = state.unifications.fresh(depth, context.prim.t);
                    let result_u = context.queries.intern_type(Type::Unification(result_u));

                    let function_u = context.intern_function(argument_u, result_u);
                    unification::solve(state, context, function_kind, unification_id, function_u)?;

                    result_u
                }

                _ => unknown(),
            }
        }

        Type::KindApplication(function, argument) => {
            let function_kind = elaborate_kind(state, context, function)?;
            let function_kind = normalise::expand(state, context, function_kind)?;

            match *context.lookup_type(function_kind) {
                Type::Forall(binder_id, inner_kind) => {
                    let binder = context.lookup_forall_binder(binder_id);
                    let argument = normalise::normalise(state, context, argument);
                    SubstituteName::one(state, context, binder.name, argument, inner_kind)?
                }
                _ => unknown(),
            }
        }

        Type::Constrained(_, _) => context.prim.t,
        Type::Forall(_, _) => context.prim.t,
        Type::Function(_, _) => context.prim.t,

        Type::Constructor(file_id, type_id) => {
            toolkit::lookup_file_type(state, context, file_id, type_id)?
        }

        Type::Integer(_) => context.prim.int,
        Type::String(_, _) => context.prim.symbol,
        Type::Kinded(_, kind) => kind,

        Type::Row(row_id) => {
            let row = context.lookup_row_type(row_id);
            let fields = Arc::clone(&row.fields);

            let field_kind = state.fresh_unification(context.queries, context.prim.t);
            let tail_kind = context.intern_application(context.prim.row, field_kind);

            for field in fields.iter() {
                let kind = elaborate_kind(state, context, field.id)?;
                unification::unify(state, context, field_kind, kind)?;
            }

            if let Some(tail) = row.tail {
                let kind = elaborate_kind(state, context, tail)?;
                unification::unify(state, context, tail_kind, kind)?;
            }

            context.intern_application(context.prim.row, field_kind)
        }

        Type::Unification(unification_id) => state.unifications.get(unification_id).kind,
        Type::Rigid(_, _, kind) => kind,
        Type::Free(_) => unknown(),
        Type::Unknown(_) => unknown(),
    };

    Ok(kind)
}
