//! Implements normalisation algorithms for the core representation.

use building_types::QueryResult;
use itertools::Itertools;
use smallvec::SmallVec;

use crate::context::CheckContext;
use crate::core::substitute::{NameToType, SubstituteName};
use crate::core::{ApplicationArgument, Type, TypeId, toolkit};
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

// Most types cannot normalise, so keeping the reduction loop out of line lets
// callers inline the flag check without carrying the loop's stack frame.
#[inline(never)]
fn normalise_head<Q>(state: &mut CheckState, context: &CheckContext<Q>, mut id: TypeId) -> TypeId
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

    for unification_id in reduction.compression {
        state.unifications.solve(unification_id, id);
    }

    id
}

/// Expands synonym constructor applications.
///
/// This function also applies normalisation using [`normalise`],
/// and should be used in checking rules where synonyms must be
/// transparent and inspected.
pub fn expand<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    mut id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    // Keeping the reduction head normalised avoids repeating the same
    // unification pruning while discovering synonym application spines.
    id = normalise(state, context, id);

    safe_loop! {
        let expanded = expand_synonym(state, context, id)?;
        if expanded != id {
            id = normalise(state, context, expanded);
            continue;
        }

        let expanded = expand_row_tail(state, context, id)?;
        if expanded == id {
            return Ok(id);
        }
        id = normalise(state, context, expanded);
    }
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
fn expand_synonym<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    // Collect the application spine in a single pass, preserving normalisation
    // along the spine. Most application heads are not synonyms, in which case
    // the collected spine is discarded without further work.
    let mut arguments: SmallVec<[ApplicationArgument; 4]> = SmallVec::new();
    let mut current = id;
    safe_loop! {
        match *context.lookup_type(current) {
            Type::Application(function, argument) => {
                arguments.push(ApplicationArgument::Type(argument));
                current = normalise(state, context, function);
            }
            Type::KindApplication(function, argument) => {
                arguments.push(ApplicationArgument::Kind(argument));
                current = normalise(state, context, function);
            }
            _ => break,
        }
    }

    let (file_id, type_id) = match *context.lookup_type(current) {
        Type::Constructor(file_id, type_id) => (file_id, type_id),
        _ => return Ok(id),
    };

    let checked_synonym = toolkit::lookup_file_synonym(state, context, file_id, type_id)?;
    let Some(checked_synonym) = checked_synonym else {
        return Ok(id);
    };

    let mut bindings = NameToType::default();
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
        bindings.insert(binder.name, argument);

        kind = inner;
    }

    // Create substitutions for type arguments.
    for parameter in &checked_synonym.parameters {
        let Some(ApplicationArgument::Type(argument)) = arguments.next() else {
            return Ok(id);
        };
        bindings.insert(parameter.name, argument);
    }

    // Apply the substitutions if there are any.
    let mut substituted = if bindings.is_empty() {
        checked_synonym.expansion
    } else {
        SubstituteName::many(state, context, &bindings, checked_synonym.expansion)?
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
