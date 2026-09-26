//! Implements searching for instance chains.

use std::collections::hash_map::Entry;
use std::rc::Rc;

use building_types::QueryResult;
use files::FileId;
use indexing::{
    DeriveId, IndexedModule, InstanceChainId, InstanceId, InstanceSourceItemId, TypeItemId,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::CheckContext;
use crate::core::constraint::{CanonicalConstraint, CanonicalConstraintId};
use crate::core::fd::{get_all_determined, get_functional_dependencies};
use crate::core::walk::{TypeWalker, WalkAction, walk_type};
use crate::core::{
    ApplicationArgument, CheckedInstance, Type, TypeId, constraint, normalise, toolkit,
};
use crate::error::ErrorKind;
use crate::state::CheckState;
use crate::{CheckedModule, ExternalQueries};

pub type InstanceChainKey = (FileId, InstanceChainId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstanceCandidateOrigin {
    Instance(FileId, InstanceId),
    Derive(FileId, DeriveId),
}

/// A candidate found for a constraint.
#[derive(Clone, Copy)]
pub struct InstanceCandidate {
    /// The syntactic ID for the instance chain.
    pub chain: Option<InstanceChainKey>,
    /// The position of the instance in the chain.
    pub position: u32,
    /// The provenance of the instance candidate.
    pub origin: InstanceCandidateOrigin,
    /// Type information about the instance.
    pub instance: CheckedInstance,
}

/// Candidate instance chains collected for a constraint, plus the unification
/// variables that prevented the search from widening to additional modules.
///
/// When `blocking` is non-empty, the candidate set may be incomplete. The
/// constraint solver would need to wait for these unification variables to
/// be truly solved before constraint solving is abandoned.
pub struct InstanceChains {
    candidates: Vec<InstanceCandidate>,
    pub blocking: Vec<u32>,
}

impl InstanceChains {
    fn new(mut candidates: Vec<InstanceCandidate>, blocking: Vec<u32>) -> InstanceChains {
        candidates.sort_by_key(|candidate| {
            (
                candidate.chain,
                candidate.position,
                candidate.chain.is_none().then_some(candidate.instance.signature),
            )
        });
        InstanceChains { candidates, blocking }
    }

    pub fn chains(&self) -> impl Iterator<Item = &[InstanceCandidate]> {
        self.candidates.chunk_by(|left, right| left.chain.is_some() && left.chain == right.chain)
    }
}

pub fn validate_declared_instance_overlap<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: InstanceSourceItemId,
    candidate_arguments: &mut FxHashMap<InstanceCandidateOrigin, Option<Vec<TypeId>>>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some((origin, instance)) = declared_instance_candidate(state, context, item_id) else {
        return Ok(());
    };

    let Some(&current_position) = context.instance_positions.get(&origin) else {
        return Ok(());
    };

    let Some(OverlappingDeclaredCandidates { matches, wanted }) =
        collect_overlapping_declared_candidates(
            state,
            context,
            origin,
            current_position,
            instance,
            candidate_arguments,
        )?
    else {
        return Ok(());
    };

    let constraint = state.canonicals.type_id(context, wanted);
    let instances = matches.iter().map(|candidate| candidate.instance.signature).collect();
    state.insert_error(ErrorKind::OverlappingInstances { constraint, instances });

    Ok(())
}

fn declared_instance_candidate<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    item_id: InstanceSourceItemId,
) -> Option<(InstanceCandidateOrigin, CheckedInstance)>
where
    Q: ExternalQueries,
{
    match item_id {
        InstanceSourceItemId::Instance(item_id) => {
            let id = context.indexed.items[item_id].id;
            let instance = state.checked.lookup_instance(id)?;
            let origin = InstanceCandidateOrigin::Instance(context.id, id);
            Some((origin, instance))
        }
        InstanceSourceItemId::Derive(item_id) => {
            let id = context.indexed.items[item_id].id;
            let instance = state.checked.lookup_derived_instance(id)?;
            let origin = InstanceCandidateOrigin::Derive(context.id, id);
            Some((origin, instance))
        }
    }
}

struct OverlappingDeclaredCandidates {
    matches: Vec<InstanceCandidate>,
    wanted: CanonicalConstraintId,
}

fn collect_overlapping_declared_candidates<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    origin: InstanceCandidateOrigin,
    origin_position: usize,
    instance: CheckedInstance,
    candidate_arguments: &mut FxHashMap<InstanceCandidateOrigin, Option<Vec<TypeId>>>,
) -> QueryResult<Option<OverlappingDeclaredCandidates>>
where
    Q: ExternalQueries,
{
    // An instance head is the dual of a constraint; we synthesise it here to reuse
    // [`collect_instance_chains`]'s file-scoped enumeration, which scopes modules
    // by walking the head's type constructors, not by matching against candidates.
    let Some(toolkit::InstanceInfo { arguments, .. }) =
        toolkit::instance_info(state, context, instance.signature, instance.resolution)?
    else {
        return Ok(None);
    };
    let (file_id, type_id) = instance.resolution;
    let arguments: Rc<[ApplicationArgument]> = Rc::from(arguments);
    let wanted = state.canonicals.intern(CanonicalConstraint {
        file_id,
        type_id,
        arguments: Rc::clone(&arguments),
    });
    let left_arguments = constraint::matching::type_arguments(&arguments);

    let search = collect_instance_chains(state, context, wanted)?;
    let current_chain = instance_candidate_chain(context, origin);

    fn is_chain_sibling(
        candidate: InstanceCandidate,
        current_chain: Option<InstanceChainKey>,
        origin: InstanceCandidateOrigin,
    ) -> bool {
        if let (Some(candidate_chain), Some(current_chain)) = (candidate.chain, current_chain) {
            candidate_chain == current_chain && candidate.origin != origin
        } else {
            false
        }
    }

    let reportable_overlap = search.chains().flatten().filter(|candidate| {
        !is_chain_sibling(**candidate, current_chain, origin)
            && should_report_overlap(context, candidate.origin, origin, origin_position)
    });
    let mut found = false;
    for candidate in reportable_overlap {
        let Some(right_arguments) =
            declared_candidate_arguments(state, context, candidate, candidate_arguments)?
        else {
            continue;
        };
        if constraint::matching::declared_instances_overlap(
            state,
            context,
            instance,
            &left_arguments,
            candidate.instance,
            right_arguments,
        )? {
            found = true;
            break;
        }
    }
    if !found {
        return Ok(None);
    }

    // Preserve the complete candidate list used by overlap diagnostics. This
    // second pass only runs for an instance that will produce a diagnostic.
    let mut matches = vec![];
    'chain: for chain in search.chains() {
        for &candidate in chain {
            if is_chain_sibling(candidate, current_chain, origin) {
                continue;
            }
            let Some(right_arguments) =
                declared_candidate_arguments(state, context, &candidate, candidate_arguments)?
            else {
                continue;
            };
            if constraint::matching::declared_instances_overlap(
                state,
                context,
                instance,
                &left_arguments,
                candidate.instance,
                right_arguments,
            )? {
                matches.push(candidate);
                continue 'chain;
            }
        }
    }

    Ok(Some(OverlappingDeclaredCandidates { matches, wanted }))
}

fn declared_candidate_arguments<'a, Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    candidate: &InstanceCandidate,
    cache: &'a mut FxHashMap<InstanceCandidateOrigin, Option<Vec<TypeId>>>,
) -> QueryResult<Option<&'a [TypeId]>>
where
    Q: ExternalQueries,
{
    let arguments = match cache.entry(candidate.origin) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let info = toolkit::instance_info(
                state,
                context,
                candidate.instance.signature,
                candidate.instance.resolution,
            )?;
            entry.insert(info.map(|info| constraint::matching::type_arguments(&info.arguments)))
        }
    };
    Ok(arguments.as_deref())
}

/// Collects [`InstanceCandidate`]s for a given constraint.
pub fn collect_instance_chains<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraint: CanonicalConstraintId,
) -> QueryResult<InstanceChains>
where
    Q: ExternalQueries,
{
    let constraint = state.canonicals[constraint].clone(); // TODO: FIXME

    let mut files = FxHashSet::from_iter([constraint.file_id]);
    let mut blocking = vec![];
    for &argument in constraint.arguments.iter() {
        let argument = match argument {
            ApplicationArgument::Kind(id) | ApplicationArgument::Type(id) => id,
        };
        CollectFileReferences::collect(state, context, argument, &mut files, &mut blocking)?;
    }

    let mut instances = vec![];

    for file_id in files {
        if file_id == context.id {
            collect_instances_from_checked(
                &mut instances,
                file_id,
                &state.checked,
                &context.indexed,
                constraint.file_id,
                constraint.type_id,
            );
        } else {
            let key = (file_id, constraint.file_id, constraint.type_id);
            let cached = context.dependency_instance_candidates.borrow();
            if let Some(candidates) = cached.get(&key) {
                instances.extend_from_slice(candidates);
                continue;
            }
            drop(cached);

            let checked = context.checked_dependency(file_id)?;
            let indexed = context.queries.indexed(file_id)?;
            let mut candidates = vec![];
            collect_instances_from_checked(
                &mut candidates,
                file_id,
                &checked,
                &indexed,
                constraint.file_id,
                constraint.type_id,
            );
            instances.extend_from_slice(&candidates);
            context.dependency_instance_candidates.borrow_mut().insert(key, candidates);
        }
    }

    Ok(InstanceChains::new(instances, blocking))
}

fn instance_candidate_chain<Q>(
    context: &CheckContext<Q>,
    origin: InstanceCandidateOrigin,
) -> Option<InstanceChainKey>
where
    Q: ExternalQueries,
{
    let InstanceCandidateOrigin::Instance(file_id, instance_id) = origin else {
        return None;
    };

    if file_id != context.id {
        return None;
    }

    context.indexed.pairs.instance_chain_id(instance_id).map(|chain_id| (file_id, chain_id))
}

fn should_report_overlap<Q>(
    context: &CheckContext<Q>,
    candidate: InstanceCandidateOrigin,
    origin: InstanceCandidateOrigin,
    origin_position: usize,
) -> bool
where
    Q: ExternalQueries,
{
    if candidate == origin {
        return false;
    }

    let Some(&candidate_position) = context.instance_positions.get(&candidate) else {
        return true;
    };

    candidate_position < origin_position
}

fn collect_instances_from_checked(
    output: &mut Vec<InstanceCandidate>,
    file_id: FileId,
    checked: &CheckedModule,
    indexed: &IndexedModule,
    class_file: FileId,
    class_id: TypeItemId,
) {
    let instances = checked
        .instances
        .iter()
        .filter(|(_, instance)| instance.resolution == (class_file, class_id))
        .map(|(&id, &instance)| {
            let (chain, position) = indexed
                .pairs
                .instance_chain_metadata(id)
                .map_or((None, 0), |(chain_id, position)| (Some((file_id, chain_id)), position));
            let origin = InstanceCandidateOrigin::Instance(file_id, id);
            InstanceCandidate { chain, position, origin, instance }
        });

    let derived = checked
        .derived_instances
        .iter()
        .filter(|(_, instance)| instance.resolution == (class_file, class_id))
        .map(|(&id, &instance)| {
            let origin = InstanceCandidateOrigin::Derive(file_id, id);
            InstanceCandidate { chain: None, position: 0, origin, instance }
        });

    output.extend(instances);
    output.extend(derived);
}

pub fn validate_rows<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_item: TypeItemId,
    arguments: &[TypeId],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let functional_dependencies =
        get_functional_dependencies(state, context, class_file, class_item)?;
    let all_determined = get_all_determined(&functional_dependencies);

    for (position, &argument_type) in arguments.iter().enumerate() {
        if all_determined.contains(&position) {
            continue;
        }

        if HasLabeledRole::contains(state, context, argument_type)? {
            let type_id = argument_type;
            state.insert_error(ErrorKind::InstanceHeadLabeledRow {
                class_file,
                class_item,
                position,
                type_id,
            });
        }
    }

    Ok(())
}

struct CollectFileReferences<'a> {
    files: &'a mut FxHashSet<FileId>,
    blocking: &'a mut Vec<u32>,
}

impl<'a> CollectFileReferences<'a> {
    fn collect<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        files: &'a mut FxHashSet<FileId>,
        blocking: &'a mut Vec<u32>,
    ) -> QueryResult<()>
    where
        Q: ExternalQueries,
    {
        let id = normalise::expand(state, context, id)?;
        walk_type(state, context, id, &mut CollectFileReferences { files, blocking })
    }
}

impl TypeWalker for CollectFileReferences<'_> {
    fn visit<Q>(
        &mut self,
        _state: &mut CheckState,
        _context: &CheckContext<Q>,
        _id: TypeId,
        t: &Type,
    ) -> QueryResult<WalkAction>
    where
        Q: ExternalQueries,
    {
        match t {
            Type::Constructor(file_id, _) => {
                self.files.insert(*file_id);
            }
            Type::Unification(id) => {
                self.blocking.push(*id);
            }
            _ => {}
        }
        Ok(WalkAction::Continue)
    }
}

struct HasLabeledRole {
    contains: bool,
}

impl HasLabeledRole {
    fn contains<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
    ) -> QueryResult<bool>
    where
        Q: ExternalQueries,
    {
        let mut walker = HasLabeledRole { contains: false };
        walk_type(state, context, id, &mut walker)?;
        Ok(walker.contains)
    }
}

impl TypeWalker for HasLabeledRole {
    fn visit<Q>(
        &mut self,
        _state: &mut CheckState,
        context: &CheckContext<Q>,
        _id: TypeId,
        t: &Type,
    ) -> QueryResult<WalkAction>
    where
        Q: ExternalQueries,
    {
        if let Type::Row(row_id) = t {
            let row = context.lookup_row_type(*row_id);
            if !row.fields.is_empty() {
                self.contains = true;
                return Ok(WalkAction::Break);
            }
        }

        Ok(WalkAction::Continue)
    }
}
