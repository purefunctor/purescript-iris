//! Implements the subtyping and unification algorithms.

use std::sync::Arc;

use building_types::QueryResult;
use itertools::{EitherOrBoth, Itertools};
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{Depth, Name, RowField, RowType, RowTypeId, Type, TypeId, normalise, toolkit};
use crate::error::ErrorKind;
use crate::evidence::EvidenceVarId;
use crate::source::types;
use crate::state::{CheckState, UnificationEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanUnify {
    Equal,
    Apart,
    Unify,
}

impl CanUnify {
    pub fn and_then(self, f: impl FnOnce() -> QueryResult<CanUnify>) -> QueryResult<CanUnify> {
        if let CanUnify::Equal = self { f() } else { Ok(self) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtypeApplication {
    Type { result: TypeId },
    Evidence { evidence: EvidenceVarId, constraint: TypeId, result: TypeId },
}

/// Strategy for handling constrained types during [`subtype_with`].
///
/// Elaboration involves pushing constraints as "wanted". This behaviour is
/// only wanted in positions where the type checker can insert dictionaries.
///
/// For example in [`Type::Function`], subtyping is [`NonElaborating`]
/// because it's impossible to insert dictionaries there; the same is
/// true when checking [`lowering::BinderKind`] in arguments and patterns.
pub trait SubtypePolicy<Q>
where
    Q: ExternalQueries,
{
    fn on_constrained(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        t1: TypeId,
        constraint: TypeId,
        constrained: TypeId,
        t2: TypeId,
    ) -> QueryResult<bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Elaborating;

impl<Q> SubtypePolicy<Q> for Elaborating
where
    Q: ExternalQueries,
{
    fn on_constrained(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        _t1: TypeId,
        constraint: TypeId,
        constrained: TypeId,
        t2: TypeId,
    ) -> QueryResult<bool> {
        state.push_wanted(constraint);
        subtype_with::<Self, Q>(state, context, constrained, t2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonElaborating;

impl<Q> SubtypePolicy<Q> for NonElaborating
where
    Q: ExternalQueries,
{
    fn on_constrained(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        t1: TypeId,
        _constraint: TypeId,
        _constrained: TypeId,
        t2: TypeId,
    ) -> QueryResult<bool> {
        unify(state, context, t1, t2)
    }
}

pub fn subtype<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    subtype_with::<Elaborating, Q>(state, context, t1, t2)
}

/// Checks subtyping and returns implicit applications introduced by outer
/// `forall` and constrained types on the inferred side.
///
/// Applications introduced while checking an expected `forall` are omitted,
/// because the inferred expression remains polymorphic at that boundary.
pub fn subtype_with_applications<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> QueryResult<Vec<SubtypeApplication>>
where
    Q: ExternalQueries,
{
    let mut applications = vec![];
    subtype_with_applications_core(state, context, t1, t2, &mut applications)?;
    Ok(applications)
}

fn subtype_with_applications_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
    applications: &mut Vec<SubtypeApplication>,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let t1 = normalise::expand(state, context, t1)?;
    let t2 = normalise::expand(state, context, t2)?;

    if t1 == t2 {
        return Ok(true);
    }

    match (context.lookup_type(t1), context.lookup_type(t2)) {
        // The inferred expression remains polymorphic at this boundary, so
        // ordinary subtyping handles it without collecting applications.
        (_, Type::Forall(_, _)) => subtype(state, context, t1, t2),
        (Type::Forall(binder_id, inner), _) => {
            let binder = context.lookup_forall_binder(*binder_id);
            let argument = state.fresh_unification(context.queries, binder.kind);
            let result = SubstituteName::one(state, context, binder.name, argument, *inner)?;
            applications.push(SubtypeApplication::Type { result });
            subtype_with_applications_core(state, context, result, t2, applications)
        }
        (Type::Constrained(constraint, result), _) => {
            let evidence = state.push_wanted(*constraint);
            applications.push(SubtypeApplication::Evidence {
                evidence,
                constraint: *constraint,
                result: *result,
            });
            subtype_with_applications_core(state, context, *result, t2, applications)
        }
        (_, _) => subtype(state, context, t1, t2),
    }
}

pub fn subtype_with<P, Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> QueryResult<bool>
where
    P: SubtypePolicy<Q>,
    Q: ExternalQueries,
{
    let t1 = normalise::expand(state, context, t1)?;
    let t2 = normalise::expand(state, context, t2)?;

    if t1 == t2 {
        return Ok(true);
    }

    let t1_core = context.lookup_type(t1);
    let t2_core = context.lookup_type(t2);

    match (t1_core, t2_core) {
        // Function subtyping is contravariant on the argument type and
        // covariant on the result type. It must also be non-elaborating
        // on both positions; it's impossible to generate any constraint
        // dictionaries at this position.
        (Type::Function(t1_argument, t1_result), Type::Function(t2_argument, t2_result)) => {
            subtype_function::<Q>(
                state,
                context,
                (*t1_argument, *t1_result),
                (*t2_argument, *t2_result),
            )
        }

        // Preserve function subtyping on the application representation.
        (Type::Application(_, _), Type::Function(t2_argument, t2_result)) => {
            if let Some((t1_argument, t1_result)) = toolkit::decompose_function(state, context, t1)?
            {
                subtype_function::<Q>(
                    state,
                    context,
                    (t1_argument, t1_result),
                    (*t2_argument, *t2_result),
                )
            } else {
                unify(state, context, t1, t2)
            }
        }
        (Type::Function(t1_argument, t1_result), Type::Application(_, _)) => {
            if let Some((t2_argument, t2_result)) = toolkit::decompose_function(state, context, t2)?
            {
                subtype_function::<Q>(
                    state,
                    context,
                    (*t1_argument, *t1_result),
                    (t2_argument, t2_result),
                )
            } else {
                unify(state, context, t1, t2)
            }
        }

        // Forall-R
        //
        //   A <: forall b. B
        //   A <: B[b := b']
        //
        // In practice, this disallows the subtyping
        //
        //   (Int -> Int) <: (forall b. b -> b)
        //   (Int -> Int) <: (b' -> b')
        //
        // The rigid variable b' is scoped within the `forall`; this is
        // enforced through unification and rigid variables carrying
        // their depth, and promote_type checking that unification
        // variables cannot be shallower than rigid variable depths.
        //
        // The classic example is `runST`
        //
        //   runST :: forall a. (forall h. ST h a) -> a
        //
        // If `?a` is solved to a type containing `h`, the skolem escape
        // check is triggered because `h` was defined in a deeper scope.
        (_, Type::Forall(binder_id, inner)) => {
            let binder = context.lookup_forall_binder(*binder_id);
            let text = state.checked.lookup_name(binder.name);
            let skolem = state.fresh_rigid_named(context.queries, binder.kind, text);

            let inner = SubstituteName::one(state, context, binder.name, skolem, *inner)?;
            state.with_depth(|state| subtype_with::<P, Q>(state, context, t1, inner))
        }

        // Forall-L
        //
        //   forall a. A <: B
        //    A[a := ?a] <: B
        //
        // In practice, this allows the subtyping
        //
        //   (forall a. a -> a) <: Int -> Int
        //           (?a -> ?a) <: Int -> Int
        //
        // with the substitution [?a := Int], and
        //
        //   (forall a. a -> a) <: (forall b. b -> b)
        //           (?a -> ?a) <: (b' -> b')
        //
        // with the substitution [?a := b'].
        (Type::Forall(binder_id, inner), _) => {
            let binder = context.lookup_forall_binder(*binder_id);
            let unification = state.fresh_unification(context.queries, binder.kind);

            let inner = SubstituteName::one(state, context, binder.name, unification, *inner)?;
            subtype_with::<P, Q>(state, context, inner, t2)
        }

        (Type::Constrained(constraint, constrained), _) => {
            P::on_constrained(state, context, t1, *constraint, *constrained, t2)
        }

        // Native computations are covariant in their effect budget. An
        // expression may require fewer effects than its surrounding context
        // permits, while the result value remains invariant.
        (Type::Application(t1_function, t1_value), Type::Application(t2_function, t2_value))
            if let (
                Type::Application(t1_constructor, t1_effects),
                Type::Application(t2_constructor, t2_effects),
            ) = (context.lookup_type(*t1_function), context.lookup_type(*t2_function))
                && t1_constructor == t2_constructor
                && (*t1_constructor == context.iris_effect.sync
                    || *t1_constructor == context.iris_effect.asynchronous)
                && !matches!(context.lookup_type(*t2_effects), Type::Unification(_)) =>
        {
            let subset = context.queries.intern_type(Type::Constructor(
                context.prim_effect.file_id,
                context.prim_effect.subset,
            ));
            let subset = context.intern_application(subset, *t1_effects);
            let subset = context.intern_application(subset, *t2_effects);
            state.push_wanted(subset);
            unify(state, context, *t1_value, *t2_value)
        }

        // Record subtyping is implemented through subtype_rows. Unlike
        // unification, the directionality of subtyping allows us to emit
        // errors like AdditionalProperty and PropertyIsMissing.
        (
            Type::Application(t1_function, t1_argument),
            Type::Application(t2_function, t2_argument),
        ) if *t1_function == context.prim.record && *t2_function == context.prim.record => {
            let t1_argument = normalise::expand(state, context, *t1_argument)?;
            let t2_argument = normalise::expand(state, context, *t2_argument)?;

            let t1_argument_core = context.lookup_type(t1_argument);
            let t2_argument_core = context.lookup_type(t2_argument);

            if let (Type::Row(t1_row_id), Type::Row(t2_row_id)) =
                (t1_argument_core, t2_argument_core)
            {
                subtype_rows::<NonElaborating, Q>(state, context, *t1_row_id, *t2_row_id)
            } else {
                unify(state, context, t1, t2)
            }
        }

        (_, _) => unify(state, context, t1, t2),
    }
}

fn subtype_function<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: (TypeId, TypeId),
    t2: (TypeId, TypeId),
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let (t1_argument, t1_result) = t1;
    let (t2_argument, t2_result) = t2;
    Ok(subtype_with::<NonElaborating, Q>(state, context, t2_argument, t1_argument)?
        && subtype_with::<NonElaborating, Q>(state, context, t1_result, t2_result)?)
}

pub fn unify<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let t1 = normalise::expand(state, context, t1)?;
    let t2 = normalise::expand(state, context, t2)?;

    if t1 == t2 {
        return Ok(true);
    }

    let t1_core = context.lookup_type(t1);
    let t2_core = context.lookup_type(t2);

    let unifies = match (t1_core, t2_core) {
        // PureScript has an impredicative type system i.e. unification
        // variables can be solved to Type::Forall. These rules must
        // be placed before the one-sided Type::Forall skolemisation.
        (Type::Unification(id), _) => return solve(state, context, t1, *id, t2),
        (_, Type::Unification(id)) => return solve(state, context, t2, *id, t1),

        (
            Type::Application(t1_function, t1_argument),
            Type::Application(t2_function, t2_argument),
        ) => {
            unify(state, context, *t1_function, *t2_function)?
                && unify(state, context, *t1_argument, *t2_argument)?
        }

        (
            Type::KindApplication(t1_function, t1_argument),
            Type::KindApplication(t2_function, t2_argument),
        ) => {
            unify(state, context, *t1_function, *t2_function)?
                && unify(state, context, *t1_argument, *t2_argument)?
        }

        // Forall-Forall
        //
        //   forall a. A ~ forall b. B
        //     A[a := c] ~ B[b := c]
        //
        // The rigid variable c is scoped within both `forall`. Unlike
        // subtyping that has directionality, unification needs both
        // types to be instantiated with the same fresh varible. It
        // does not make sense for Forall to instantiate either side.
        (Type::Forall(t1_binder_id, t1_inner), Type::Forall(t2_binder_id, t2_inner)) => {
            let t1_binder = context.lookup_forall_binder(*t1_binder_id);
            let t2_binder = context.lookup_forall_binder(*t2_binder_id);

            unify(state, context, t1_binder.kind, t2_binder.kind)?;

            let text = state
                .checked
                .lookup_name(t1_binder.name)
                .or_else(|| state.checked.lookup_name(t2_binder.name));
            let skolem = state.fresh_rigid_named(context.queries, t1_binder.kind, text);

            let t1_inner = SubstituteName::one(state, context, t1_binder.name, skolem, *t1_inner)?;
            let t2_inner = SubstituteName::one(state, context, t2_binder.name, skolem, *t2_inner)?;

            state.with_depth(|state| unify(state, context, t1_inner, t2_inner))?
        }

        // Forall-B, A-Forall
        //
        // The Type::Unification pattern above is an early catch-all that
        // enables impredicativity. In practice, this unifies the following:
        //
        //   (forall a. a -> a) ~ (?b -> ?b)
        //           (a' -> a') ~ (?b -> ?b)
        //
        // with the substitution [?b := 'a], and
        //
        //   (?a -> ?a) ~ (forall b. b -> b)
        //   (?a -> ?a) ~ (b' -> b')
        //
        // with the substitution [?a := 'b].
        (Type::Forall(binder_id, inner), _) => {
            let binder = context.lookup_forall_binder(*binder_id);
            let text = state.checked.lookup_name(binder.name);
            let skolem = state.fresh_rigid_named(context.queries, binder.kind, text);
            let inner = SubstituteName::one(state, context, binder.name, skolem, *inner)?;
            unify(state, context, inner, t2)?
        }
        (_, Type::Forall(binder_id, inner)) => {
            let binder = context.lookup_forall_binder(*binder_id);
            let text = state.checked.lookup_name(binder.name);
            let skolem = state.fresh_rigid_named(context.queries, binder.kind, text);
            let inner = SubstituteName::one(state, context, binder.name, skolem, *inner)?;
            unify(state, context, t1, inner)?
        }

        (
            Type::Constrained(t1_constraint, t1_inner),
            Type::Constrained(t2_constraint, t2_inner),
        ) => {
            unify(state, context, *t1_constraint, *t2_constraint)?
                && unify(state, context, *t1_inner, *t2_inner)?
        }

        (Type::Application(_, _), Type::Function(t2_argument, t2_result)) => {
            let t2 = context.intern_function_application(*t2_argument, *t2_result);
            unify(state, context, t1, t2)?
        }
        (Type::Function(t1_argument, t1_result), Type::Application(_, _)) => {
            let t1 = context.intern_function_application(*t1_argument, *t1_result);
            unify(state, context, t1, t2)?
        }

        (Type::Function(t1_argument, t1_result), Type::Function(t2_argument, t2_result)) => {
            unify(state, context, *t1_argument, *t2_argument)?
                && unify(state, context, *t1_result, *t2_result)?
        }

        (Type::Kinded(t1_inner, t1_kind), Type::Kinded(t2_inner, t2_kind)) => {
            unify(state, context, *t1_inner, *t2_inner)?
                && unify(state, context, *t1_kind, *t2_kind)?
        }

        (Type::Constructor(t1_file, t1_item), Type::Constructor(t2_file, t2_item))
            if t1_file == t2_file && t1_item == t2_item =>
        {
            true
        }

        (Type::Integer(t1_value), Type::Integer(t2_value)) if t1_value == t2_value => true,

        (Type::String(t1_kind, t1_value), Type::String(t2_kind, t2_value))
            if t1_kind == t2_kind && t1_value == t2_value =>
        {
            true
        }

        (Type::Row(t1_row_id), Type::Row(t2_row_id)) => {
            unify_rows(state, context, *t1_row_id, *t2_row_id)?
        }

        (Type::Rigid(t1_name, _, t1_kind), Type::Rigid(t2_name, _, t2_kind))
            if t1_name == t2_name =>
        {
            unify(state, context, *t1_kind, *t2_kind)?
        }

        _ => false,
    };

    if !unifies {
        state.insert_error(ErrorKind::CannotUnify { t1, t2 });
    }

    Ok(unifies)
}

/// Determines if two types [`CanUnify`].
///
/// This is a pure, speculative version of [`unify`] that does not perform
/// any side effects. It is used in instance matching to check whether two
/// types bound to the same rigid variable can actually unify before
/// generating an equality.
pub fn can_unify<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: TypeId,
    t2: TypeId,
) -> QueryResult<CanUnify>
where
    Q: ExternalQueries,
{
    let t1 = normalise::expand(state, context, t1)?;
    let t2 = normalise::expand(state, context, t2)?;

    if t1 == t2 {
        return Ok(CanUnify::Equal);
    }

    let t1_core = context.lookup_type(t1);
    let t2_core = context.lookup_type(t2);

    match (t1_core, t2_core) {
        (Type::Unification(_), _) | (_, Type::Unification(_)) => Ok(CanUnify::Unify),
        (Type::Unknown(_), _) | (_, Type::Unknown(_)) => Ok(CanUnify::Unify),
        (Type::Row(_), Type::Row(_)) => Ok(CanUnify::Unify),

        (
            Type::Application(t1_function, t1_argument),
            Type::Application(t2_function, t2_argument),
        ) => can_unify(state, context, *t1_function, *t2_function)?
            .and_then(|| can_unify(state, context, *t1_argument, *t2_argument)),

        (Type::Function(t1_argument, t1_result), Type::Function(t2_argument, t2_result)) => {
            can_unify(state, context, *t1_argument, *t2_argument)?
                .and_then(|| can_unify(state, context, *t1_result, *t2_result))
        }

        (Type::Application(t1_function, _), Type::Function(t2_argument, t2_result)) => {
            let t1_function = normalise::expand(state, context, *t1_function)?;
            if matches!(context.lookup_type(t1_function), Type::Application(_, _)) {
                let t2 = context.intern_function_application(*t2_argument, *t2_result);
                can_unify(state, context, t1, t2)
            } else {
                Ok(CanUnify::Apart)
            }
        }
        (Type::Function(t1_argument, t1_result), Type::Application(t2_function, _)) => {
            let t2_function = normalise::expand(state, context, *t2_function)?;
            if matches!(context.lookup_type(t2_function), Type::Application(_, _)) {
                let t1 = context.intern_function_application(*t1_argument, *t1_result);
                can_unify(state, context, t1, t2)
            } else {
                Ok(CanUnify::Apart)
            }
        }

        (
            Type::KindApplication(t1_function, t1_argument),
            Type::KindApplication(t2_function, t2_argument),
        ) => can_unify(state, context, *t1_function, *t2_function)?
            .and_then(|| can_unify(state, context, *t1_argument, *t2_argument)),

        (Type::Kinded(t1_inner, t1_kind), Type::Kinded(t2_inner, t2_kind)) => {
            can_unify(state, context, *t1_inner, *t2_inner)?
                .and_then(|| can_unify(state, context, *t1_kind, *t2_kind))
        }

        (
            Type::Constrained(t1_constraint, t1_constrained),
            Type::Constrained(t2_constraint, t2_constrained),
        ) => can_unify(state, context, *t1_constraint, *t2_constraint)?
            .and_then(|| can_unify(state, context, *t1_constrained, *t2_constrained)),

        (Type::Rigid(t1_name, _, t1_kind), Type::Rigid(t2_name, _, t2_kind)) => {
            if t1_name == t2_name {
                can_unify(state, context, *t1_kind, *t2_kind)
            } else {
                Ok(CanUnify::Apart)
            }
        }

        (Type::Forall(t1_binder, t1_inner), Type::Forall(t2_binder, t2_inner)) => {
            let t1_binder = context.lookup_forall_binder(*t1_binder);
            let t2_binder = context.lookup_forall_binder(*t2_binder);
            can_unify(state, context, t1_binder.kind, t2_binder.kind)?
                .and_then(|| can_unify(state, context, *t1_inner, *t2_inner))
        }

        _ => Ok(CanUnify::Apart),
    }
}

/// Solves a unification variable to a given type.
pub fn solve<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    unification: TypeId,
    id: u32,
    solution: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let solution = normalise::expand(state, context, solution)?;

    match promote_type(state, context, id, solution)? {
        PromoteResult::Ok => {}
        PromoteResult::OccursCheck | PromoteResult::SkolemEscape => {
            let t1 = unification;
            let t2 = solution;
            state.insert_error(ErrorKind::CannotUnify { t1, t2 });
            return Ok(false);
        }
    }

    let unification_kind = state.unifications.get(id).kind;
    let solution_kind = types::elaborate_kind(state, context, solution)?;
    unify(state, context, unification_kind, solution_kind)?;

    state.transfer_effect_origins(unification, solution);
    state.unifications.solve(id, solution);
    Ok(true)
}

enum PromoteResult {
    Ok,
    OccursCheck,
    SkolemEscape,
}

impl PromoteResult {
    fn and_then(
        self,
        f: impl FnOnce() -> QueryResult<PromoteResult>,
    ) -> QueryResult<PromoteResult> {
        match self {
            PromoteResult::Ok => f(),
            result => Ok(result),
        }
    }
}

/// Checks that solving a unification variable is safe.
///
/// This function has several responsibilities:
/// - the [occurs check], where a solution that contains the unification
///   variable being solved is rejected. This avoids infinite types;
/// - the [skolem escape check], where a solution that contains a rigid
///   type variable deeper than the unification variable is rejected;
/// - lastly, promotion which solves deeper unification variables into
///   fresh unification variables as shallow as the one being solved.
///
/// Since PureScript is an impredicative language i.e. it allows unification
/// variables to be solved to [`Type::Forall`], we keep track of the names
/// bound in the context specifically to allow `?t := forall a. a -> a` to
/// skip the skolem escape check completely.
///
/// [occurs check]: PromoteResult::OccursCheck
/// [skolem escape check]: PromoteResult::SkolemEscape
fn promote_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: u32,
    solution: TypeId,
) -> QueryResult<PromoteResult>
where
    Q: ExternalQueries,
{
    struct PromotionState {
        id: u32,
        depth: Depth,
        names: Vec<Name>,
    }

    fn check<Q>(
        promote: &mut PromotionState,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
    ) -> QueryResult<PromoteResult>
    where
        Q: ExternalQueries,
    {
        let id = normalise::expand(state, context, id)?;
        let t = context.lookup_type(id);

        match t {
            Type::Application(function, argument) | Type::KindApplication(function, argument) => {
                check(promote, state, context, *function)?
                    .and_then(|| check(promote, state, context, *argument))
            }

            Type::Forall(binder_id, inner) => {
                let binder = context.lookup_forall_binder(*binder_id);

                let on_kind = check(promote, state, context, binder.kind)?;
                if !matches!(on_kind, PromoteResult::Ok) {
                    return Ok(on_kind);
                }

                promote.names.push(binder.name);
                let on_inner = check(promote, state, context, *inner)?;
                promote.names.pop();

                Ok(on_inner)
            }

            Type::Constrained(constraint, inner) => check(promote, state, context, *constraint)?
                .and_then(|| check(promote, state, context, *inner)),

            Type::Function(argument, result) => check(promote, state, context, *argument)?
                .and_then(|| check(promote, state, context, *result)),

            Type::Kinded(inner, kind) => check(promote, state, context, *inner)?
                .and_then(|| check(promote, state, context, *kind)),

            Type::Constructor(_, _) => Ok(PromoteResult::Ok),
            Type::Integer(_) | Type::String(_, _) => Ok(PromoteResult::Ok),

            Type::Row(row_id) => {
                let row = context.lookup_row_type(*row_id);
                for field in row.fields.iter() {
                    let on_field = check(promote, state, context, field.id)?;
                    if !matches!(on_field, PromoteResult::Ok) {
                        return Ok(on_field);
                    }
                }
                if let Some(tail) = row.tail {
                    check(promote, state, context, tail)
                } else {
                    Ok(PromoteResult::Ok)
                }
            }

            Type::Rigid(name, rigid_depth, kind) => {
                if promote.names.contains(name) {
                    check(promote, state, context, *kind)
                } else if *rigid_depth > promote.depth {
                    Ok(PromoteResult::SkolemEscape)
                } else {
                    check(promote, state, context, *kind)
                }
            }

            Type::Unification(id) => {
                // Disallow `?t := ?t`
                if *id == promote.id {
                    return Ok(PromoteResult::OccursCheck);
                }
                // When solving `a` to a deeper unification variable `b`,
                //
                //   ?a[0] := ?b[1]
                //
                // we create a fresh variable `c` as shallow as `a`,
                //
                //   ?b[1] := ?c[0]
                //
                // which would be pruned into the final solution
                //
                //   ?a[0] := ?c[0]
                //
                // This process eliminates unsolved unification variables
                // at depth markers that are not in scope, and replaces
                // them with unification variables that are in scope.
                let UnificationEntry { depth, kind, .. } = *state.unifications.get(*id);
                if depth > promote.depth {
                    let promoted = state.unifications.fresh(promote.depth, kind);
                    let promoted = context.queries.intern_type(Type::Unification(promoted));
                    state.unifications.solve(*id, promoted);
                }

                Ok(PromoteResult::Ok)
            }

            Type::Free(_) | Type::Unknown(_) => Ok(PromoteResult::Ok),
        }
    }

    let depth = state.unifications.get(id).depth;
    let names = vec![];

    let mut promote = PromotionState { id, depth, names };
    check(&mut promote, state, context, solution)
}

/// Unification on row types.
///
/// Algorithmically, `t1 ~ t2` checks shared labels field-by-field via [`unify`].
/// - if both are closed, then additional fields are disallowed
/// - if `t1` is closed, additional fields in `t2` are disallowed
/// - if `t2` is closed, additional fields in `t1` are disallowed
/// - finally, [`unify_row_tails`] handles the remaining cases
fn unify_rows<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: RowTypeId,
    t2: RowTypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let t1_row = context.lookup_row_type(t1);
    let t2_row = context.lookup_row_type(t2);

    let (left_only, right_only, ok) = partition_row_fields(state, context, t1_row, t2_row)?;

    if !ok {
        return Ok(false);
    }

    if t1_row.tail.is_none() && t2_row.tail.is_none() {
        return Ok(left_only.is_empty() && right_only.is_empty());
    }

    if t2_row.tail.is_none() && !left_only.is_empty() {
        return Ok(false);
    }

    if t1_row.tail.is_none() && !right_only.is_empty() {
        return Ok(false);
    }

    unify_row_tails(state, context, t1_row.tail, t2_row.tail, left_only, right_only)
}

/// Like [`unify_rows`], but uses [`subtype`] for partitioning.
///
/// Additionally, the directionality of the [`subtype`] relation enables the
/// addition of [`PropertyIsMissing`] and [`AdditionalProperty`] error when
/// dealing with closed rows.
///
/// Algorithmically, `t1 <: t2` checks shared labels field-by-field via [`subtype`].
///
/// Extra labels are only rejected when they appear against a closed row:
/// - if `t1` is closed, labels exclusive to `t2` are [`PropertyIsMissing`];
/// - if `t2` is closed, labels exclusive to `t1` are [`AdditionalProperty`].
/// - finally, [`unify_row_tails`] handles the remaining cases
///
/// [`PropertyIsMissing`]: ErrorKind::PropertyIsMissing
/// [`AdditionalProperty`]: ErrorKind::AdditionalProperty
fn subtype_rows<P, Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1: RowTypeId,
    t2: RowTypeId,
) -> QueryResult<bool>
where
    P: SubtypePolicy<Q>,
    Q: ExternalQueries,
{
    let t1_row = context.lookup_row_type(t1);
    let t2_row = context.lookup_row_type(t2);

    let (left_only, right_only, ok) = partition_row_fields_with(
        state,
        context,
        t1_row,
        t2_row,
        |state, context, left, right| subtype_with::<P, Q>(state, context, left, right),
    )?;

    if !ok {
        return Ok(false);
    }

    let mut failed = false;

    if t1_row.tail.is_none() && !right_only.is_empty() {
        let labels = Arc::from_iter(right_only.iter().map(|field| SmolStr::clone(&field.label)));
        state.insert_error(ErrorKind::PropertyIsMissing { labels });
        failed = true;
    }

    if t2_row.tail.is_none() && !left_only.is_empty() {
        let labels = Arc::from_iter(left_only.iter().map(|field| SmolStr::clone(&field.label)));
        state.insert_error(ErrorKind::AdditionalProperty { labels });
        failed = true;
    }

    if failed {
        return Ok(false);
    }

    unify_row_tails(state, context, t1_row.tail, t2_row.tail, left_only, right_only)
}

fn partition_row_fields<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1_row: &RowType,
    t2_row: &RowType,
) -> QueryResult<(Vec<RowField>, Vec<RowField>, bool)>
where
    Q: ExternalQueries,
{
    partition_row_fields_with(state, context, t1_row, t2_row, unify)
}

fn partition_row_fields_with<Q, F>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1_row: &RowType,
    t2_row: &RowType,
    mut field_check: F,
) -> QueryResult<(Vec<RowField>, Vec<RowField>, bool)>
where
    Q: ExternalQueries,
    F: FnMut(&mut CheckState, &CheckContext<Q>, TypeId, TypeId) -> QueryResult<bool>,
{
    let mut extras_left = vec![];
    let mut extras_right = vec![];
    let mut ok = true;

    let t1_fields = t1_row.fields.iter();
    let t2_fields = t2_row.fields.iter();

    for field in t1_fields.merge_join_by(t2_fields, |left, right| left.label.cmp(&right.label)) {
        match field {
            EitherOrBoth::Both(left, right) => {
                if !field_check(state, context, left.id, right.id)? {
                    ok = false;
                }
            }
            EitherOrBoth::Left(left) => extras_left.push(left.clone()),
            EitherOrBoth::Right(right) => extras_right.push(right.clone()),
        }
    }

    Ok((extras_left, extras_right, ok))
}

fn unify_row_tails<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    t1_tail: Option<TypeId>,
    t2_tail: Option<TypeId>,
    extras_left: Vec<RowField>,
    extras_right: Vec<RowField>,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    match (t1_tail, t2_tail) {
        (None, None) => Ok(true),

        // t1_tail ~ ( ...extras_right )
        (Some(t1_tail), None) => {
            let row_type = context.intern_row(extras_right, None);
            unify(state, context, t1_tail, row_type)
        }

        // ( ...extras_left ) ~ t2_tail
        (None, Some(t2_tail)) => {
            let row_type = context.intern_row(extras_left, None);
            unify(state, context, t2_tail, row_type)
        }

        // ( | t1_tail ) ~ ( | t2_tail )
        //
        // If no additional fields
        //
        //   t1_tail ~ t2_tail
        //
        // with additional fields
        //
        //   t1_tail ~ ( ...extras_right | ?tail )
        //   t2_tail ~ (  ...extras_left | ?tail )
        //
        (Some(t1_tail), Some(t2_tail)) => {
            if extras_left.is_empty() && extras_right.is_empty() {
                return unify(state, context, t1_tail, t2_tail);
            }

            let tail_kind = state.fresh_unification(context.queries, context.prim.t);
            let tail = state.fresh_unification(context.queries, tail_kind);

            let left_tail_row_type = context.intern_row(extras_right, Some(tail));
            let right_tail_row_type = context.intern_row(extras_left, Some(tail));

            Ok(unify(state, context, t1_tail, left_tail_row_type)?
                && unify(state, context, t2_tail, right_tail_row_type)?)
        }
    }
}
