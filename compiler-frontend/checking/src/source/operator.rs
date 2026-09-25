//! Implements surface-generic operator chain inference.

use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use lowering::IsElement;
use sugar::OperatorTree;
use sugar::bracketing::BracketingResult;

use crate::context::CheckContext;
use crate::core::{Type, TypeId, normalise, toolkit, unification};
use crate::source::types::application;
use crate::source::{binder, synonym, terms, types};
use crate::state::CheckState;
use crate::{ExternalQueries, OperatorBranchTypes, tree};

#[derive(Copy, Clone, Debug)]
enum OperatorKindMode {
    Infer,
    Check { expected_type: TypeId },
}

pub struct OperatorApplication<Elaborated> {
    implicit: Vec<terms::application::ImplicitApplication>,
    argument: (Elaborated, TypeId),
    result_type: TypeId,
}

pub struct OperatorBranch<OperatorId, Item, Elaborated> {
    operator_id: OperatorId,
    operator: ((FileId, Item), TypeId),
    left: OperatorApplication<Elaborated>,
    right: OperatorApplication<Elaborated>,
}

pub fn infer_operator_chain<Q, E>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: E,
) -> QueryResult<(E::Elaborated, TypeId)>
where
    Q: ExternalQueries,
    E: IsOperator<Q>,
{
    let unknown = || (E::unknown_elaborated(context), context.unknown("invalid operator chain"));

    let Some(operator_tree) = E::lookup_tree(context, id) else {
        return Ok(unknown());
    };

    let Ok(operator_tree) = operator_tree else {
        return Ok(unknown());
    };

    traverse_operator_tree(state, context, operator_tree, OperatorKindMode::Infer)
}

pub fn check_operator_chain<Q, E>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: E,
    expected_type: TypeId,
) -> QueryResult<(E::Elaborated, TypeId)>
where
    Q: ExternalQueries,
    E: IsOperator<Q>,
{
    let unknown = || (E::unknown_elaborated(context), expected_type);

    let Some(operator_tree) = E::lookup_tree(context, id) else {
        return Ok(unknown());
    };

    let Ok(operator_tree) = operator_tree else {
        return Ok(unknown());
    };

    traverse_operator_tree(state, context, operator_tree, OperatorKindMode::Check { expected_type })
}

fn traverse_operator_tree<Q, E>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    operator_tree: &OperatorTree<E>,
    mode: OperatorKindMode,
) -> QueryResult<(E::Elaborated, TypeId)>
where
    Q: ExternalQueries,
    E: IsOperator<Q>,
{
    if let OperatorKindMode::Check { expected_type } = mode {
        return E::check_expected_subtree(state, context, expected_type, |state, expected_type| {
            traverse_operator_tree_core(
                state,
                context,
                operator_tree,
                OperatorKindMode::Check { expected_type },
            )
        });
    }

    traverse_operator_tree_core(state, context, operator_tree, mode)
}

fn traverse_operator_tree_core<Q, E>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    operator_tree: &OperatorTree<E>,
    mode: OperatorKindMode,
) -> QueryResult<(E::Elaborated, TypeId)>
where
    Q: ExternalQueries,
    E: IsOperator<Q>,
{
    let unknown_elaborated = || E::unknown_elaborated(context);

    match operator_tree {
        OperatorTree::Leaf(None) => match mode {
            OperatorKindMode::Infer => {
                Ok((unknown_elaborated(), context.unknown("missing operator leaf")))
            }
            OperatorKindMode::Check { expected_type } => Ok((unknown_elaborated(), expected_type)),
        },

        OperatorTree::Leaf(Some(type_id)) => match mode {
            OperatorKindMode::Infer => E::infer_surface(state, context, *type_id),
            OperatorKindMode::Check { expected_type } => {
                // Peel constraints from the expected type as givens,
                // so operator arguments like `unsafePartial $ expr`
                // can discharge constraints like Partial properly.
                let expected_type = toolkit::collect_givens(state, context, expected_type)?;
                E::check_surface(state, context, *type_id, expected_type)
            }
        },

        OperatorTree::Branch(operator_id, children) => {
            let Some((file_id, item_id)) = E::lookup_operator(context, *operator_id) else {
                return match mode {
                    OperatorKindMode::Infer => {
                        Ok((unknown_elaborated(), context.unknown("missing operator resolution")))
                    }
                    OperatorKindMode::Check { expected_type } => {
                        Ok((unknown_elaborated(), expected_type))
                    }
                };
            };

            let operator_type = E::lookup_item(state, context, file_id, item_id)?;

            traverse_operator_branch(
                state,
                context,
                *operator_id,
                (file_id, item_id),
                operator_type,
                children,
                mode,
            )
        }
    }
}

fn traverse_operator_branch<Q, E>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    operator_id: E::OperatorId,
    operator: (FileId, E::ItemId),
    operator_type: TypeId,
    children: &[OperatorTree<E>; 2],
    mode: OperatorKindMode,
) -> QueryResult<(E::Elaborated, TypeId)>
where
    Q: ExternalQueries,
    E: IsOperator<Q>,
{
    let unknown = || match mode {
        OperatorKindMode::Infer => {
            (E::unknown_elaborated(context), context.unknown("invalid operator kind"))
        }
        OperatorKindMode::Check { expected_type } => {
            (E::unknown_elaborated(context), expected_type)
        }
    };

    let Some(terms::application::UnanchoredApplication {
        implicit: left_implicit,
        argument: left_type,
        result: right_function_type,
    }) = terms::application::check_unanchored_application(state, context, operator_type)?
    else {
        return Ok(unknown());
    };

    let Some(terms::application::UnanchoredApplication {
        implicit: right_implicit,
        argument: right_type,
        result: result_type,
    }) = terms::application::check_unanchored_application(state, context, right_function_type)?
    else {
        return Ok(unknown());
    };

    E::record_branch_types(state, operator_id, left_type, right_type, result_type);

    let check_left_right = |state: &mut CheckState| {
        let [left_tree, right_tree] = children;

        let (left, _) = traverse_operator_tree(
            state,
            context,
            left_tree,
            OperatorKindMode::Check { expected_type: left_type },
        )?;

        let (right, _) = traverse_operator_tree(
            state,
            context,
            right_tree,
            OperatorKindMode::Check { expected_type: right_type },
        )?;

        Ok((left, right))
    };

    let (left, right) = if E::should_defer_expansion(state, context, operator)? {
        state.with_defer_expansion(check_left_right)?
    } else {
        check_left_right(state)?
    };

    if let OperatorKindMode::Check { expected_type } = mode {
        // Peel constraints from the expected type as givens,
        // so operator result constraints can be discharged.
        let expected_type = toolkit::collect_givens(state, context, expected_type)?;
        let _ = unification::subtype(state, context, result_type, expected_type)?;
    }

    let branch = OperatorBranch {
        operator_id,
        operator: (operator, operator_type),
        left: OperatorApplication {
            implicit: left_implicit,
            argument: (left, left_type),
            result_type: right_function_type,
        },
        right: OperatorApplication {
            implicit: right_implicit,
            argument: (right, right_type),
            result_type,
        },
    };
    E::build(state, context, branch)
}

pub trait IsOperator<Q: ExternalQueries>: IsElement {
    type ItemId: Copy;
    type Elaborated: Copy;

    fn unknown_elaborated(context: &CheckContext<Q>) -> Self::Elaborated;

    fn lookup_tree<'q>(
        context: &'q CheckContext<Q>,
        id: Self,
    ) -> Option<&'q BracketingResult<Self>>;

    fn lookup_operator(
        context: &CheckContext<Q>,
        id: Self::OperatorId,
    ) -> Option<(FileId, Self::ItemId)>;

    fn lookup_item(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> QueryResult<TypeId>;

    fn infer_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
    ) -> QueryResult<(Self::Elaborated, TypeId)>;

    fn check_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
        expected: TypeId,
    ) -> QueryResult<(Self::Elaborated, TypeId)>;

    fn check_expected_subtree<F>(
        state: &mut CheckState,
        _context: &CheckContext<Q>,
        expected: TypeId,
        check: F,
    ) -> QueryResult<(Self::Elaborated, TypeId)>
    where
        F: FnOnce(&mut CheckState, TypeId) -> QueryResult<(Self::Elaborated, TypeId)>,
    {
        check(state, expected)
    }

    fn should_defer_expansion(
        _state: &CheckState,
        _context: &CheckContext<Q>,
        _operator: (FileId, Self::ItemId),
    ) -> QueryResult<bool> {
        Ok(false)
    }

    fn build(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        branch: OperatorBranch<Self::OperatorId, Self::ItemId, Self::Elaborated>,
    ) -> QueryResult<(Self::Elaborated, TypeId)>;

    fn record_branch_types(
        state: &mut CheckState,
        operator_id: Self::OperatorId,
        left: TypeId,
        right: TypeId,
        result: TypeId,
    );
}

impl<Q: ExternalQueries> IsOperator<Q> for lowering::ExpressionId {
    type ItemId = TermItemId;
    type Elaborated = Option<terms::ElaboratedExpression>;

    fn unknown_elaborated(_context: &CheckContext<Q>) -> Self::Elaborated {
        None
    }

    fn lookup_tree<'q>(
        context: &'q CheckContext<Q>,
        id: Self,
    ) -> Option<&'q BracketingResult<Self>> {
        context.bracketed.expressions.get(&id)
    }

    fn lookup_operator(
        context: &CheckContext<Q>,
        id: Self::OperatorId,
    ) -> Option<(FileId, Self::ItemId)> {
        context.lowered.tree.get_term_operator(id)
    }

    fn lookup_item(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> QueryResult<TypeId> {
        toolkit::lookup_file_term_operator(state, context, file_id, item_id)
    }

    fn infer_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let inferred = terms::infer_expression(state, context, id)?;
        Ok((Some(inferred), inferred.type_id))
    }

    fn check_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
        expected: TypeId,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let checked = terms::check_expression(state, context, id, expected)?;
        Ok((Some(checked), checked.type_id))
    }

    fn check_expected_subtree<F>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        expected: TypeId,
        check: F,
    ) -> QueryResult<(Self::Elaborated, TypeId)>
    where
        F: FnOnce(&mut CheckState, TypeId) -> QueryResult<(Self::Elaborated, TypeId)>,
    {
        let expected = normalise::expand(state, context, expected)?;
        let Type::Constrained(constraint, constrained) = *context.lookup_type(expected) else {
            return check(state, expected);
        };

        state.with_implication(|state| {
            let binder = state.push_given(constraint);
            let (checked, _) = Self::check_expected_subtree(state, context, constrained, check)?;

            let checked = checked.map(|checked| {
                let kind = tree::ExpressionKind::EvidenceAbstraction {
                    binder,
                    expression: checked.expression,
                };
                terms::allocate_expression(state, expected, kind)
            });

            Ok((checked, expected))
        })
    }

    fn build(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        OperatorBranch { operator: (operator, operator_type), left, right, .. }: OperatorBranch<
            Self::OperatorId,
            Self::ItemId,
            Self::Elaborated,
        >,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let (file_id, item_id) = operator;
        let (Some(left_argument), _) = left.argument else {
            return Ok((None, right.result_type));
        };
        let (Some(right_argument), _) = right.argument else {
            return Ok((None, right.result_type));
        };
        let Some((target_file_id, target_item_id)) =
            toolkit::resolve_term_operator_target(context, file_id, item_id)?
        else {
            return Ok((None, right.result_type));
        };

        let operator = terms::allocate_term_reference(
            state,
            context,
            target_file_id,
            target_item_id,
            operator_type,
        )?;
        let left = terms::application::materialize_application(
            state,
            operator,
            left.implicit,
            left.result_type,
            left_argument,
        );
        let expression = terms::application::materialize_application(
            state,
            left,
            right.implicit,
            right.result_type,
            right_argument,
        );
        Ok((Some(expression), right.result_type))
    }

    fn record_branch_types(
        state: &mut CheckState,
        operator_id: Self::OperatorId,
        left: TypeId,
        right: TypeId,
        result: TypeId,
    ) {
        state
            .checked
            .node_types
            .term_operators
            .insert(operator_id, OperatorBranchTypes { left, right, result });
    }
}

impl<Q: ExternalQueries> IsOperator<Q> for lowering::TypeId {
    type ItemId = indexing::TypeItemId;
    type Elaborated = TypeId;

    fn unknown_elaborated(context: &CheckContext<Q>) -> Self::Elaborated {
        context.unknown("invalid operator chain")
    }

    fn lookup_tree<'q>(
        context: &'q CheckContext<Q>,
        id: Self,
    ) -> Option<&'q BracketingResult<Self>> {
        context.bracketed.types.get(&id)
    }

    fn lookup_operator(
        context: &CheckContext<Q>,
        id: Self::OperatorId,
    ) -> Option<(FileId, Self::ItemId)> {
        context.lowered.tree.get_type_operator(id)
    }

    fn lookup_item(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> QueryResult<TypeId> {
        toolkit::lookup_file_type_operator(state, context, file_id, item_id)
    }

    fn infer_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        types::infer_kind(state, context, id)
    }

    fn check_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
        expected: TypeId,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        types::check_kind(state, context, id, expected)
    }

    fn should_defer_expansion(
        state: &CheckState,
        context: &CheckContext<Q>,
        (file_id, item_id): (FileId, Self::ItemId),
    ) -> QueryResult<bool> {
        let Some((target_file_id, target_item_id)) =
            toolkit::resolve_type_operator_target(context, file_id, item_id)?
        else {
            return Ok(false);
        };
        Ok(toolkit::lookup_file_synonym(state, context, target_file_id, target_item_id)?.is_some())
    }

    fn build(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        OperatorBranch { operator: (operator, _), left, right, .. }: OperatorBranch<
            Self::OperatorId,
            Self::ItemId,
            Self::Elaborated,
        >,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let (file_id, item_id) = operator;
        let (left_argument, left_kind) = left.argument;
        let (right_argument, right_kind) = right.argument;
        let Some((target_file_id, target_item_id)) =
            toolkit::resolve_type_operator_target(context, file_id, item_id)?
        else {
            let unknown = context.unknown("missing operator resolution");
            return Ok((unknown, unknown));
        };

        let operator_kind = toolkit::lookup_file_type(state, context, file_id, item_id)?;

        if let Some((elaborated_type, result_kind)) = synonym::try_check_synonym_application(
            state,
            context,
            (target_file_id, target_item_id),
            operator_kind,
            &[(left_argument, left_kind), (right_argument, right_kind)],
        )? {
            let result_kind = normalise::normalise(state, context, result_kind);
            return Ok((elaborated_type, result_kind));
        }

        let function_type = Type::Constructor(target_file_id, target_item_id);
        let function_type = context.queries.intern_type(function_type);

        let function: application::FnTypeKind = (function_type, operator_kind);
        let arguments = [
            application::Argument::Core(left_argument, left_kind),
            application::Argument::Core(right_argument, right_kind),
        ];

        let ((elaborated_type, _), _) = application::infer_application_arguments(
            state,
            context,
            function,
            &arguments,
            application::Options::OPERATOR,
            application::Records::Ignore,
        )?;

        let result_kind = normalise::normalise(state, context, right.result_type);
        Ok((elaborated_type, result_kind))
    }

    fn record_branch_types(
        state: &mut CheckState,
        operator_id: Self::OperatorId,
        left: TypeId,
        right: TypeId,
        result: TypeId,
    ) {
        state
            .checked
            .node_types
            .type_operators
            .insert(operator_id, OperatorBranchTypes { left, right, result });
    }
}

impl<Q: ExternalQueries> IsOperator<Q> for lowering::BinderId {
    type ItemId = TermItemId;
    type Elaborated = Option<binder::ElaboratedBinder>;

    fn unknown_elaborated(_context: &CheckContext<Q>) -> Self::Elaborated {
        None
    }

    fn lookup_tree<'q>(
        context: &'q CheckContext<Q>,
        id: Self,
    ) -> Option<&'q BracketingResult<Self>> {
        context.bracketed.binders.get(&id)
    }

    fn lookup_operator(
        context: &CheckContext<Q>,
        id: Self::OperatorId,
    ) -> Option<(FileId, Self::ItemId)> {
        context.lowered.tree.get_term_operator(id)
    }

    fn lookup_item(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        file_id: FileId,
        item_id: Self::ItemId,
    ) -> QueryResult<TypeId> {
        toolkit::lookup_file_term_operator(state, context, file_id, item_id)
    }

    fn infer_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let inferred = binder::infer_binder(state, context, id)?;
        Ok((Some(inferred), inferred.type_id))
    }

    fn check_surface(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: Self,
        expected: TypeId,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let checked = binder::check_binder(state, context, id, expected)?;
        Ok((Some(checked), checked.type_id))
    }

    fn build(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        OperatorBranch { operator_id, operator: (operator, _), left, right }: OperatorBranch<
            Self::OperatorId,
            Self::ItemId,
            Self::Elaborated,
        >,
    ) -> QueryResult<(Self::Elaborated, TypeId)> {
        let (file_id, item_id) = operator;
        let (Some(left), _) = left.argument else {
            return Ok((None, right.result_type));
        };
        let (Some(right_argument), _) = right.argument else {
            return Ok((None, right.result_type));
        };
        let Some(resolution) = toolkit::resolve_term_operator_target(context, file_id, item_id)?
        else {
            return Ok((None, right.result_type));
        };

        let arguments = [left.binder, right_argument.binder];
        let kind = tree::BinderKind::Constructor { resolution, arguments: Arc::from(arguments) };
        let binder = state.allocate_operator_binder(operator_id, right.result_type, kind);
        let elaborated = binder::ElaboratedBinder { type_id: right.result_type, binder };
        Ok((Some(elaborated), right.result_type))
    }

    fn record_branch_types(
        state: &mut CheckState,
        operator_id: Self::OperatorId,
        left: TypeId,
        right: TypeId,
        result: TypeId,
    ) {
        state
            .checked
            .node_types
            .term_operators
            .insert(operator_id, OperatorBranchTypes { left, right, result });
    }
}
