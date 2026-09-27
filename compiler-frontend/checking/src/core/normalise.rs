//! Implements normalisation algorithms for the core representation.

use building_types::QueryResult;
use itertools::Itertools;
use smallvec::SmallVec;

use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{ApplicationArgument, Name, Type, TypeFlags, TypeId, toolkit};
use crate::state::{CheckState, UnificationState};
use crate::{ExternalQueries, safe_loop};

struct ReductionContext<'a, 'q, Q>
where
    Q: ExternalQueries,
{
    state: &'a mut CheckState,
    context: &'a CheckContext<'q, Q>,
    compression: SmallVec<[u32; 4]>,
}

impl<'a, 'q, Q> ReductionContext<'a, 'q, Q>
where
    Q: ExternalQueries,
{
    fn new(state: &'a mut CheckState, context: &'a CheckContext<'q, Q>) -> Self {
        ReductionContext { state, context, compression: SmallVec::new() }
    }

    fn reduce_once(&mut self, id: TypeId) -> Option<TypeId> {
        let t = self.context.lookup_type(id);

        if let Some(next) = self.rule_prune_unifications(t) {
            return Some(next);
        }
        if let Some(next) = self.rule_simplify_rows(t) {
            return Some(next);
        }

        None
    }

    fn rule_prune_unifications(&mut self, t: &Type) -> Option<TypeId> {
        let Type::Unification(unification_id) = *t else {
            return None;
        };

        let UnificationState::Solved(solution_id) =
            self.state.unifications.get(unification_id).state
        else {
            return None;
        };

        self.compression.push(unification_id);
        Some(solution_id)
    }

    fn rule_simplify_rows(&self, t: &Type) -> Option<TypeId> {
        let Type::Row(row_id) = *t else {
            return None;
        };

        let row = self.context.lookup_row_type(row_id);

        let tail_id = row.tail?;
        let tail_t = self.context.lookup_type(tail_id);

        let Type::Row(inner_row_id) = *tail_t else {
            return None;
        };

        if inner_row_id == row_id {
            return None;
        }

        let inner = self.context.lookup_row_type(inner_row_id);

        let merged_fields = {
            let left = row.fields.iter().cloned();
            let right = inner.fields.iter().cloned();
            left.merge_by(right, |left, right| left.label <= right.label)
        };

        Some(self.context.intern_row(merged_fields, inner.tail))
    }
}

/// Normalises a [`Type`] head.
///
/// Notably, this function applies the following rules:
/// 1. Replaces solved unfiication variables, compressing them
///    if they solve to other solved unification variables.
/// 2. Simplifies row types by merging concrete row tails.
///
/// This function should be used in checking rules where
/// synonyms must remain opaque such as in kind checking.
#[inline]
pub fn normalise<Q>(state: &mut CheckState, context: &CheckContext<Q>, id: TypeId) -> TypeId
where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(id).may_normalise() {
        return id;
    }
    normalise_head(state, context, id)
}

/// Normalises a [`Type`] head that was already looked up with its flags.
///
/// Traversals inspect [`TypeFlags`] before normalising, so this reuses that
/// lookup and only looks up the [`Type`] again when normalisation changed it.
#[inline]
pub fn normalise_looked_up<'q, Q>(
    state: &mut CheckState,
    context: &CheckContext<'q, Q>,
    id: TypeId,
    t: &'q Type,
    flags: TypeFlags,
) -> (TypeId, &'q Type)
where
    Q: ExternalQueries,
{
    if !flags.may_normalise() {
        return (id, t);
    }
    let normalised = normalise_head(state, context, id);
    if normalised == id { (id, t) } else { (normalised, context.lookup_type(normalised)) }
}

// Most types cannot normalise, so keeping the reduction loop out of line lets
// callers inline the flag check without carrying the loop's stack frame.
#[inline(never)]
pub(crate) fn normalise_head<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    // Most heads are a unification variable that is either unsolved or solved
    // to a type that cannot normalise further, which needs no compression.
    if let Type::Unification(unification_id) = *context.lookup_type(id) {
        match state.unifications.get(unification_id).state {
            UnificationState::Unsolved => return id,
            UnificationState::Solved(solution_id)
                if !context.lookup_type_flags(solution_id).may_normalise() =>
            {
                return solution_id;
            }
            UnificationState::Solved(_) => {}
        }
    }

    reduce_head(state, context, id)
}

// Chains of solutions and nested rows are rare, so keeping their reduction
// loop separate keeps the common case above free of its stack frame.
#[inline(never)]
fn reduce_head<Q>(state: &mut CheckState, context: &CheckContext<Q>, mut id: TypeId) -> TypeId
where
    Q: ExternalQueries,
{
    let mut reduction = ReductionContext::new(state, context);

    let id = safe_loop! {
        if let Some(reduced_id) = reduction.reduce_once(id) {
            id = reduced_id;
        } else {
            break id;
        }
    };

    // Most chains are a single solved unification that already points at the
    // normal form; only longer chains benefit from rewriting their solutions.
    for unification_id in reduction.compression {
        if state.unifications.get(unification_id).state != UnificationState::Solved(id) {
            state.unifications.solve(unification_id, id);
        }
    }

    id
}

/// Expands synonym constructor applications.
///
/// This function also applies normalisation using [`normalise`],
/// and should be used in checking rules where synonyms must be
/// transparent and inspected.
#[inline(always)]
pub fn expand<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    // Normalisation only rewrites unification variable and row heads, so any
    // other head that cannot expand is already final without memoisation.
    let (t, flags) = context.lookup_type_with_flags(id);
    if !matches!(t, Type::Unification(_)) && !may_expand(t) {
        return Ok(id);
    }
    if let Some(expanded) = state.lookup_recent_expansion(id) {
        return Ok(expanded);
    }
    expand_looked_up(state, context, id, t, flags)
}

// Many expanded types have heads that cannot expand, so keeping the rest out
// of line lets callers inline that check without paying for a call.
#[inline(never)]
fn expand_looked_up<'q, Q>(
    state: &mut CheckState,
    context: &CheckContext<'q, Q>,
    id: TypeId,
    t: &'q Type,
    flags: TypeFlags,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    // Most unification variable heads are unsolved, or solved to a type that
    // cannot normalise further, whose expansion is then also the expansion of
    // the variable and may already be memoised.
    if let Type::Unification(unification_id) = *t {
        match state.unifications.get(unification_id).state {
            UnificationState::Unsolved => return Ok(id),
            UnificationState::Solved(solution_id) => {
                let (solution_t, solution_flags) = context.lookup_type_with_flags(solution_id);
                if !solution_flags.may_normalise() {
                    if !may_expand(solution_t) {
                        return Ok(solution_id);
                    }
                    return expand(state, context, solution_id);
                }
            }
        }
    }

    // Unification variables may be solved between calls, so only expansions
    // that neither start from nor lead to one are stable enough to memoise.
    if flags.has_unification() {
        return expand_uncached(state, context, id, t, flags);
    }

    if let Some(expanded) = state.lookup_expansion_cache(id) {
        return Ok(expanded);
    }

    let expanded = expand_uncached(state, context, id, t, flags)?;
    if !context.lookup_type_flags(expanded).has_unification() {
        state.insert_expansion_cache(id, expanded);
    }

    Ok(expanded)
}

fn expand_uncached<'q, Q>(
    state: &mut CheckState,
    context: &CheckContext<'q, Q>,
    id: TypeId,
    t: &'q Type,
    flags: TypeFlags,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    // Keeping the reduction head normalised avoids repeating the same
    // unification pruning while discovering synonym application spines.
    let (mut id, mut t) = normalise_looked_up(state, context, id, t, flags);

    safe_loop! {
        if !may_expand(t) {
            return Ok(id);
        }
        let expanded = if let Type::Row(_) = t {
            expand_row_tail(state, context, id)?
        } else {
            expand_synonym(state, context, id, t)?
        };
        if expanded == id {
            return Ok(id);
        }
        let (expanded_t, flags) = context.lookup_type_with_flags(expanded);
        (id, t) = normalise_looked_up(state, context, expanded, expanded_t, flags);
    }
}

/// Whether [`expand`] may rewrite a normalised type with this head.
///
/// Synonyms are only reachable through a constructor, possibly applied, and
/// rows may have tails that expand into further rows; every other head, such
/// as an unsolved unification variable, is final.
fn may_expand(t: &Type) -> bool {
    matches!(
        t,
        Type::Application(..) | Type::KindApplication(..) | Type::Constructor(..) | Type::Row(_)
    )
}

fn expand_row_tail<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let Type::Row(_) = context.lookup_type(id) else {
        return Ok(id);
    };

    let mut current_id = id;
    let mut row_fields = Vec::new();

    // This flag tracks that we've flattened at least one row tail.
    // If we have, we will build a row from the collected fields;
    // otherwise, we need to return the original row type.
    let mut flattened_once = false;

    let row_tail = safe_loop! {
        let Type::Row(row_id) = *context.lookup_type(current_id) else {
            if flattened_once {
                break Some(current_id);
            } else {
                return Ok(id);
            }
        };

        let row = context.lookup_row_type(row_id);

        let Some(original_tail) = row.tail else {
            if flattened_once {
                row_fields.extend(row.fields.iter().cloned());
                break None;
            } else {
                return Ok(id);
            }
        };

        let normalised_tail = expand(state, context, original_tail)?;

        if original_tail == normalised_tail {
            if flattened_once {
                row_fields.extend(row.fields.iter().cloned());
                break Some(original_tail);
            } else {
                return Ok(id);
            }
        }

        row_fields.extend(row.fields.iter().cloned());
        current_id = normalised_tail;
        flattened_once = true;
    };

    Ok(context.intern_row(row_fields, row_tail))
}

/// Expands synonym constructor applications with respect to oversaturation.
///
/// In certain cases, type synonyms can be oversaturated or applied with more
/// arguments than they're declared to accept. In the following example:
///
/// ```purescript
/// type Identity :: forall k. k -> k
/// type Identity a = a
///
/// data Tuple a b = Tuple a b
///
/// test1 :: Identity Array Int
/// test1 = [42]
///
/// test2 :: Identity Tuple Int String
/// test2 = Tuple 42 "hello"
///
/// forceSolve = { test1, test2 }
/// ```
///
/// The `Identity Array` and `Identity Tuple` will be expanded to reveal
/// `Array` and `Tuple` which are applied to their respective arguments.
fn expand_synonym<'q, Q>(
    state: &mut CheckState,
    context: &CheckContext<'q, Q>,
    id: TypeId,
    t: &'q Type,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    // Collect the application spine in a single pass, preserving normalisation
    // along the spine. Most application heads are not synonyms, in which case
    // the collected spine is discarded without further work.
    let mut arguments: SmallVec<[ApplicationArgument; 4]> = SmallVec::new();
    let mut head_id = id;
    let mut head = t;
    let mut normalised_spine = false;
    safe_loop! {
        let (function, argument) = match *head {
            Type::Application(function, argument) => {
                (function, ApplicationArgument::Type(argument))
            }
            Type::KindApplication(function, argument) => {
                (function, ApplicationArgument::Kind(argument))
            }
            _ => break,
        };
        arguments.push(argument);
        let (function_t, flags) = context.lookup_type_with_flags(function);
        (head_id, head) = normalise_looked_up(state, context, function, function_t, flags);
        normalised_spine |= head_id != function;
    }

    let Type::Constructor(file_id, type_id) = *head else {
        return Ok(id);
    };

    let checked_synonym = if state.is_known_not_synonym(head_id) {
        None
    } else {
        let checked_synonym = toolkit::lookup_file_synonym(state, context, file_id, type_id)?;
        if checked_synonym.is_none() {
            state.insert_known_not_synonym(head_id);
        }
        checked_synonym
    };

    let Some(checked_synonym) = checked_synonym else {
        // Reaching the head without normalising means the spine has no
        // unification variable in function position, so this application
        // expands to itself whatever its arguments are solved to, for as long
        // as its head is not a synonym.
        if !normalised_spine {
            state.insert_recent_expansion(id, id);
        }
        return Ok(id);
    };

    let mut bindings: SmallVec<[(Name, TypeId); 4]> = SmallVec::new();
    let mut kind = checked_synonym.kind;
    arguments.reverse();
    let mut arguments = arguments.into_iter();

    // Create substitutions for kind arguments. For example,
    //
    //   type T :: forall k. k -> Type
    //   type T (a :: k) = Proxy (a :: k)
    //
    // given an application such as,
    //
    //   T @Type Int
    //
    // this loop produces the replacement,
    //
    //   k := Type
    //
    // which is later substituted into the synonym body. Without this step,
    // expansion would leave `k` rigid inside the synonym body causing
    // unification errors downstream.
    safe_loop! {
        kind = normalise(state, context, kind);

        let Type::Forall(binder_id, inner) = *context.lookup_type(kind) else {
            break;
        };

        let Some(ApplicationArgument::Kind(argument)) = arguments.next() else {
            return Ok(id);
        };

        let binder = context.lookup_forall_binder(binder_id);
        bindings.push((binder.name, argument));

        kind = inner;
    }

    // Create substitutions for type arguments.
    for parameter in &checked_synonym.parameters {
        let Some(ApplicationArgument::Type(argument)) = arguments.next() else {
            return Ok(id);
        };
        bindings.push((parameter.name, argument));
    }

    // Apply the substitutions if there are any.
    let mut substituted = if bindings.is_empty() {
        checked_synonym.expansion
    } else {
        SubstituteName::few(state, context, &bindings, checked_synonym.expansion)?
    };

    // Reconstruct applications from remaining oversaturated arguments.
    for argument in arguments {
        substituted = match argument {
            ApplicationArgument::Type(argument) => {
                context.intern_application(substituted, argument)
            }
            ApplicationArgument::Kind(argument) => {
                context.intern_kind_application(substituted, argument)
            }
        };
    }

    Ok(substituted)
}
