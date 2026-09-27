//! Implements canonicalisation for constraints.

use std::ops::Index;
use std::rc::Rc;

use building_types::QueryResult;
use files::FileId;
use indexing::TypeItemId;
use interner::{Id, Interner};
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::context::CheckContext;
use crate::core::substitute::{NameToType, SubstituteName};
use crate::core::{ApplicationArgument, Type, TypeId, toolkit, zonk};
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

/// The canonical structure of a constraint.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CanonicalConstraint {
    pub file_id: FileId,
    pub type_id: TypeItemId,
    pub arguments: Rc<[ApplicationArgument]>,
}

impl CanonicalConstraint {
    pub fn expect_type_arguments<const N: usize>(&self) -> Option<[TypeId; N]> {
        self.arguments
            .iter()
            .filter_map(|argument| match argument {
                ApplicationArgument::Type(argument) => Some(*argument),
                ApplicationArgument::Kind(_) => None,
            })
            .collect_array()
    }
}

/// Stable identifier for a [`CanonicalConstraint`].
pub type CanonicalConstraintId = Id<CanonicalConstraint>;

/// Interner and cache for [`CanonicalConstraint`].
#[derive(Default)]
pub struct Canonicals {
    interner: Interner<CanonicalConstraint>,
    cache: FxHashMap<TypeId, CanonicalConstraintId>,
}

impl Canonicals {
    pub fn intern(&mut self, canonical: CanonicalConstraint) -> Id<CanonicalConstraint> {
        self.interner.intern(canonical)
    }

    pub fn type_id<Q>(&self, context: &CheckContext<Q>, id: CanonicalConstraintId) -> TypeId
    where
        Q: ExternalQueries,
    {
        let CanonicalConstraint { file_id, type_id, arguments } = &self[id];
        let mut constraint = context.queries.intern_type(Type::Constructor(*file_id, *type_id));

        for &argument in arguments.iter() {
            constraint = match argument {
                ApplicationArgument::Kind(argument) => {
                    context.intern_kind_application(constraint, argument)
                }
                ApplicationArgument::Type(argument) => {
                    context.intern_application(constraint, argument)
                }
            };
        }

        constraint
    }

    /// Looks up the memoised canonical form of a constraint type.
    ///
    /// Only constraint types without unification variables are memoised; their
    /// canonical form depends solely on the synonyms in scope, which
    /// [`CheckState::insert_synonym`] invalidates through [`Canonicals::clear_cache`].
    ///
    /// [`CheckState::insert_synonym`]: crate::state::CheckState::insert_synonym
    fn lookup(&self, constraint: TypeId) -> Option<CanonicalConstraintId> {
        self.cache.get(&constraint).copied()
    }

    fn associate(
        &mut self,
        constraint: TypeId,
        canonical: CanonicalConstraint,
    ) -> CanonicalConstraintId {
        let id = self.intern(canonical);
        let previous = self.cache.insert(constraint, id);
        debug_assert!(previous.is_none(), "critical violation: canonical cache overwrite");
        id
    }

    pub(crate) fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

impl Index<CanonicalConstraintId> for Canonicals {
    type Output = CanonicalConstraint;

    fn index(&self, index: CanonicalConstraintId) -> &CanonicalConstraint {
        &self.interner[index]
    }
}

/// Extracts a [`CanonicalConstraint`] from a [`Type`].
pub fn canonicalise<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<CanonicalConstraintId>>
where
    Q: ExternalQueries,
{
    // Canonicalisation looks through solved unification variables, so only
    // constraint types without any have a canonical form stable enough to memoise.
    let is_stable = !context.lookup_type_flags(id).has_unification();
    if is_stable && let Some(canonical_id) = state.canonicals.lookup(id) {
        return Ok(Some(canonical_id));
    }

    // The head is already expanded by extracting the applications.
    let (class, arguments) = toolkit::extract_all_applications(state, context, id)?;

    let Type::Constructor(file_id, type_id) = *context.lookup_type(class) else {
        return Ok(None);
    };

    let arguments = Rc::from(arguments.as_slice()); // TODO: extract_all_applications
    let canonical = CanonicalConstraint { file_id, type_id, arguments };
    let canonical_id = if is_stable {
        state.canonicals.associate(id, canonical)
    } else {
        state.canonicals.intern(canonical)
    };

    Ok(Some(canonical_id))
}

pub fn zonk_canonical<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: CanonicalConstraintId,
) -> QueryResult<CanonicalConstraintId>
where
    Q: ExternalQueries,
{
    let canonical = state.canonicals[id].clone();
    let arguments = canonical.arguments.iter().map(|&argument| match argument {
        ApplicationArgument::Kind(argument) => {
            zonk::zonk(state, context, argument).map(ApplicationArgument::Kind)
        }
        ApplicationArgument::Type(argument) => {
            zonk::zonk(state, context, argument).map(ApplicationArgument::Type)
        }
    });

    let arguments = arguments.collect::<QueryResult<Rc<[_]>>>()?;
    Ok(state.canonicals.intern(CanonicalConstraint { arguments, ..canonical }))
}

pub fn substitute_canonical<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &NameToType,
    id: CanonicalConstraintId,
) -> QueryResult<CanonicalConstraintId>
where
    Q: ExternalQueries,
{
    if substitution.is_empty() {
        return Ok(id);
    }

    let canonical = state.canonicals[id].clone();
    let arguments = canonical.arguments.iter().copied().map(|argument| match argument {
        ApplicationArgument::Kind(argument) => {
            substitute_type(state, context, substitution, argument).map(ApplicationArgument::Kind)
        }
        ApplicationArgument::Type(argument) => {
            substitute_type(state, context, substitution, argument).map(ApplicationArgument::Type)
        }
    });

    let arguments = arguments.collect::<QueryResult<Rc<[_]>>>()?;
    Ok(state.canonicals.intern(CanonicalConstraint { arguments, ..canonical }))
}

pub fn substitute_canonicals<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &NameToType,
    ids: &[CanonicalConstraintId],
) -> QueryResult<Vec<CanonicalConstraintId>>
where
    Q: ExternalQueries,
{
    ids.iter().copied().map(|id| substitute_canonical(state, context, substitution, id)).collect()
}

fn substitute_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    substitution: &NameToType,
    mut id: TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    safe_loop! {
        let substituted = SubstituteName::many(state, context, substitution, id)?;
        if substituted == id {
            return Ok(id);
        }
        id = substituted;
    }
}
