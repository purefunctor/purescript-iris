//! Implements searching for instance chains.

use std::rc::Rc;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{
    DeriveId, IndexedModule, InstanceChainId, InstanceId, InstanceSourceItemId, TypeItemId,
};
use rustc_hash::FxHashSet;

use crate::context::CheckContext;
use crate::core::constraint::{CanonicalConstraint, CanonicalConstraintId};
use crate::core::fd::{self, Positions, get_all_determined, get_functional_dependencies};
use crate::core::walk::{TypeWalker, WalkAction, walk_type};
use crate::core::{
    ApplicationArgument, CheckedInstance, InstanceHead, Type, TypeId, constraint, normalise,
    toolkit,
};
use crate::error::{ConstraintErrorKind, InstanceErrorKind};
use crate::state::CheckState;
use crate::{CheckedModule, ExternalQueries};

pub type InstanceChainKey = (FileId, InstanceChainId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstanceCandidateOrigin {
    Instance(FileId, InstanceId),
    Derive(FileId, DeriveId),
}

/// A candidate found for a constraint.
#[derive(Clone)]
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

pub fn checked_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: (FileId, TypeItemId),
    signature: TypeId,
    matchable: TypeId,
) -> QueryResult<CheckedInstance>
where
    Q: ExternalQueries,
{
    let head = instance_head(state, context, matchable, resolution)?.map(Arc::new);
    Ok(CheckedInstance { resolution, signature, matchable, head })
}

pub(crate) fn instance_head<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    matchable: TypeId,
    resolution: (FileId, TypeItemId),
) -> QueryResult<Option<InstanceHead>>
where
    Q: ExternalQueries,
{
    let Some(toolkit::InstanceInfo { binders, constraints, arguments }) =
        toolkit::instance_info(state, context, matchable, resolution)?
    else {
        return Ok(None);
    };
    let pattern_variables = binders.iter().map(|binder| binder.name).collect();
    let type_arguments = constraint::matching::type_arguments(&arguments);
    Ok(Some(InstanceHead { binders, pattern_variables, constraints, arguments, type_arguments }))
}

pub fn validate_declared_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: InstanceSourceItemId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some((origin, instance)) = declared_instance_candidate(state, context, item_id) else {
        return Ok(());
    };

    let Some(head) = &instance.head else {
        return Ok(());
    };

    if toolkit::contains_unknown(state, context, instance.signature)? {
        state.absorb_unknown(instance.signature);
        return Ok(());
    }

    let derives_from_data = match item_id {
        InstanceSourceItemId::Instance(_) => false,
        InstanceSourceItemId::Derive(item_id) => {
            context.lowered.tree.get_derive_item(item_id).is_some_and(|item| {
                !matches!(item.kind, lowering::DeriveItemKind::Declaration { newtype: true, .. })
            })
        }
    };

    if !derives_from_data && is_orphan_instance(state, context, instance.resolution, head)? {
        if let Some(constraint) = signature_head_constraint(state, context, &instance)? {
            state.insert_error(InstanceErrorKind::OrphanInstance { constraint });
        }
        return Ok(());
    }

    let Some(&current_position) = context.instance_positions.get(&origin) else {
        return Ok(());
    };

    let Some(matches) = collect_overlapping_declared_candidates(
        state,
        context,
        origin,
        current_position,
        instance.resolution,
        head,
    )?
    else {
        return Ok(());
    };

    let Some(constraint) = signature_head_constraint(state, context, &instance)? else {
        return Ok(());
    };
    let instances = matches.iter().map(|candidate| candidate.instance.signature).collect();
    state.insert_error(ConstraintErrorKind::OverlappingInstances { constraint, instances });

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

fn is_orphan_instance<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: (FileId, TypeItemId),
    head: &InstanceHead,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let (class_file, class_id) = resolution;
    if class_file == context.id {
        return Ok(false);
    }

    let Some(class) = toolkit::lookup_file_class(state, context, class_file, class_id)? else {
        return Ok(false);
    };

    let type_arguments = &head.type_arguments;
    if type_arguments.len() != class.type_parameters.len() {
        return Ok(false);
    }

    let mut foreign_positions = Positions::default();
    for (position, &argument) in type_arguments.iter().enumerate() {
        if head_constructor_file(state, context, argument)? != Some(context.id) {
            foreign_positions.insert(position);
        }
    }

    let all_positions = Positions::all(type_arguments.len());
    Ok(fd::positions_cover_all(&class.functional_dependencies, &foreign_positions, &all_positions))
}

fn signature_head_constraint<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    instance: &CheckedInstance,
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    let Some(toolkit::InstanceInfo { arguments, .. }) =
        toolkit::instance_info(state, context, instance.signature, instance.resolution)?
    else {
        return Ok(None);
    };

    let (file_id, type_id) = instance.resolution;
    let arguments = Rc::from(arguments);
    let head = state.canonicals.intern(CanonicalConstraint { file_id, type_id, arguments });

    Ok(Some(state.canonicals.type_id(context, head)))
}

fn head_constructor_file<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<FileId>>
where
    Q: ExternalQueries,
{
    let (function, _) = toolkit::extract_type_application(state, context, id)?;
    match *context.lookup_type(function) {
        Type::Constructor(file_id, _) | Type::Synonym(file_id, _) => Ok(Some(file_id)),
        Type::Kinded(inner, _) => head_constructor_file(state, context, inner),
        _ => Ok(None),
    }
}

fn collect_overlapping_declared_candidates<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    origin: InstanceCandidateOrigin,
    origin_position: usize,
    resolution: (FileId, TypeItemId),
    head: &InstanceHead,
) -> QueryResult<Option<Vec<InstanceCandidate>>>
where
    Q: ExternalQueries,
{
    // An instance head is the dual of a constraint; we synthesise it here to reuse
    // [`collect_instance_chains`]'s file-scoped enumeration, which scopes modules
    // by walking the head's type constructors, not by matching against candidates.
    let (file_id, type_id) = resolution;
    let arguments = Rc::from(head.arguments.as_slice());
    let wanted = state.canonicals.intern(CanonicalConstraint { file_id, type_id, arguments });

    let search = collect_instance_chains(state, context, wanted)?;
    let current_chain = instance_candidate_chain(context, origin);
    let functional_dependencies = get_functional_dependencies(state, context, file_id, type_id)?;

    fn is_chain_sibling(
        candidate: &InstanceCandidate,
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
        !is_chain_sibling(candidate, current_chain, origin)
            && should_report_overlap(context, candidate.origin, origin, origin_position)
    });
    let mut found = false;
    for candidate in reportable_overlap {
        let Some(candidate_head) = &candidate.instance.head else {
            continue;
        };
        if constraint::matching::declared_instances_overlap(
            state,
            context,
            &functional_dependencies,
            &head.type_arguments,
            &candidate_head.type_arguments,
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
    let mut matches = Vec::new();
    'chain: for chain in search.chains() {
        for candidate in chain {
            if is_chain_sibling(candidate, current_chain, origin) {
                continue;
            }
            let Some(candidate_head) = &candidate.instance.head else {
                continue;
            };
            if constraint::matching::declared_instances_overlap(
                state,
                context,
                &functional_dependencies,
                &head.type_arguments,
                &candidate_head.type_arguments,
            )? {
                matches.push(candidate.clone());
                continue 'chain;
            }
        }
    }

    Ok(Some(matches))
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
    let mut blocking = Vec::new();
    for &argument in constraint.arguments.iter() {
        let argument = match argument {
            ApplicationArgument::Kind(id) | ApplicationArgument::Type(id) => id,
        };
        CollectFileReferences::collect(state, context, argument, &mut files, &mut blocking)?;
    }

    let mut instances = Vec::new();

    for file_id in files {
        if file_id == context.id {
            let mut candidates = Vec::new();
            collect_instances_from_checked(
                &mut candidates,
                file_id,
                &state.checked,
                &context.indexed,
                constraint.file_id,
                constraint.type_id,
            );
            let candidates = without_unknown_chains(state, context, candidates)?;
            instances.extend(candidates);
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
            let mut candidates = Vec::new();
            collect_instances_from_checked(
                &mut candidates,
                file_id,
                &checked,
                &indexed,
                constraint.file_id,
                constraint.type_id,
            );
            let candidates = without_unknown_chains(state, context, candidates)?;
            instances.extend_from_slice(&candidates);
            context.dependency_instance_candidates.borrow_mut().insert(key, candidates);
        }
    }

    Ok(InstanceChains::new(instances, blocking))
}

fn without_unknown_chains<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    candidates: Vec<InstanceCandidate>,
) -> QueryResult<Vec<InstanceCandidate>>
where
    Q: ExternalQueries,
{
    let mut unknown_chains = FxHashSet::default();
    let mut unknown_origins = FxHashSet::default();
    for candidate in &candidates {
        if toolkit::contains_unknown(state, context, candidate.instance.signature)? {
            state.absorb_unknown(candidate.instance.signature);
            unknown_origins.insert(candidate.origin);
            if let Some(chain) = candidate.chain {
                unknown_chains.insert(chain);
            }
        }
    }

    let candidates = candidates.into_iter().filter(|candidate| {
        let in_unknown_chain = candidate.chain.is_some_and(|chain| unknown_chains.contains(&chain));
        !in_unknown_chain && !unknown_origins.contains(&candidate.origin)
    });

    Ok(candidates.collect())
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
        .map(|(&id, instance)| {
            let instance = instance.clone();
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
        .map(|(&id, instance)| {
            let instance = instance.clone();
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
        if all_determined.contains(position) {
            continue;
        }

        if HasLabeledRole::contains(state, context, argument_type)? {
            let type_id = argument_type;
            state.insert_error(InstanceErrorKind::InstanceHeadLabeledRow {
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
            Type::Constructor(file_id, _) | Type::Synonym(file_id, _) => {
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
