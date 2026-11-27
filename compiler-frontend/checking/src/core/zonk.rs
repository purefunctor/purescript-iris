use std::mem;
use std::sync::Arc;

use building_types::QueryResult;
use smol_str::SmolStr;

use crate::context::CheckContext;
use crate::core::fold::{FoldAction, TypeFold, fold_type};
use crate::core::{Type, TypeFlags, TypeId};
use crate::error::{
    CapabilityBundleErrorKind, CapabilityErrorKind, CheckingError, ConstraintErrorKind,
    DeriveErrorKind, ErrorKind, InstanceErrorKind, TypeErrorKind, UnificationErrorKind,
};
use crate::holes::{HoleBinding, TermHole, TypeHole};
use crate::state::CheckState;
use crate::tree::{
    BinderKind, DeclarationAbstraction, InstanceImplementation, TermDeclarationKind,
    TypeDeclarationKind,
};
use crate::{ExternalQueries, OperatorBranchTypes, holes};

struct Zonk;

impl TypeFold for Zonk {
    fn may_change(&self, flags: TypeFlags) -> bool {
        flags.may_zonk()
    }

    fn transform<Q>(
        &mut self,
        state: &mut CheckState,
        _context: &CheckContext<Q>,
        id: TypeId,
        _t: &Type,
    ) -> QueryResult<FoldAction>
    where
        Q: ExternalQueries,
    {
        let cached = state.lookup_zonk_cache(id);
        Ok(cached.map_or(FoldAction::Continue, FoldAction::Replace))
    }

    fn complete<Q>(
        &mut self,
        state: &mut CheckState,
        _context: &CheckContext<Q>,
        id: TypeId,
        result: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        state.insert_zonk_cache(id, result);
        Ok(result)
    }
}

pub fn zonk<Q>(state: &mut CheckState, context: &CheckContext<Q>, id: TypeId) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    fold_type(state, context, id, &mut Zonk)
}

/// Zonks a slice of [`TypeId`], returning [`None`] when every element is
/// unchanged so callers can preserve the existing [`Arc`] allocation.
///
/// Preserving [`Arc`] identity keeps downstream structural-equality checks
/// cheap, since [`Arc::eq`] short-circuits on pointer equality.
fn zonk_types<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    types: &[TypeId],
) -> QueryResult<Option<Arc<[TypeId]>>>
where
    Q: ExternalQueries,
{
    let mut zonked: Option<Vec<TypeId>> = None;
    for (index, &id) in types.iter().enumerate() {
        let zonked_id = zonk(state, context, id)?;
        if zonked_id != id {
            let output = zonked.get_or_insert_with(|| {
                let mut output = Vec::with_capacity(types.len());
                output.extend_from_slice(&types[..index]);
                output
            });
            output.push(zonked_id);
        } else if let Some(output) = &mut zonked {
            output.push(id);
        }
    }
    Ok(zonked.map(Into::into))
}

fn zonk_declaration_abstractions<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    abstractions: &mut Arc<[DeclarationAbstraction]>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for abstraction in Arc::make_mut(abstractions) {
        match abstraction {
            DeclarationAbstraction::Type { binder, rigid } => {
                let mut forall_binder = context.lookup_forall_binder(*binder);
                forall_binder.kind = zonk(state, context, forall_binder.kind)?;
                *binder = context.intern_forall_binder(forall_binder);
                *rigid = zonk(state, context, *rigid)?;
            }
            DeclarationAbstraction::Evidence { constraint, .. } => {
                *constraint = zonk(state, context, *constraint)?;
            }
            DeclarationAbstraction::Argument => {}
        }
    }
    Ok(())
}

pub fn zonk_nodes<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    macro_rules! zonk_type_map {
        ($field:ident) => {
            let mut node_types = mem::take(&mut state.checked.node_types.$field);
            for type_id in node_types.values_mut() {
                *type_id = zonk(state, context, *type_id)?;
            }
            state.checked.node_types.$field = node_types;
        };
    }

    macro_rules! zonk_operator_map {
        ($field:ident) => {
            let mut operator_types = mem::take(&mut state.checked.node_types.$field);
            for branch_types in operator_types.values_mut() {
                *branch_types = zonk_operator_branch(state, context, *branch_types)?;
            }
            state.checked.node_types.$field = operator_types;
        };
    }

    zonk_type_map!(types);
    zonk_type_map!(type_kinds);
    zonk_type_map!(expressions);
    zonk_type_map!(binders);
    zonk_type_map!(lets);
    zonk_type_map!(puns);
    zonk_type_map!(record_access_labels);
    zonk_type_map!(record_fields);
    zonk_type_map!(record_updates);
    zonk_type_map!(sections);
    zonk_type_map!(forall_bindings);
    zonk_type_map!(implicit_bindings);
    zonk_operator_map!(term_operators);
    zonk_operator_map!(type_operators);

    Ok(())
}

pub fn zonk_tree<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut expressions = mem::take(&mut state.checked.tree.arena.expressions);
    for (_, expression) in expressions.iter_mut() {
        expression.type_id = zonk(state, context, expression.type_id)?;
        if let crate::tree::ExpressionKind::EvidenceApplication { constraint, .. } =
            &mut expression.kind
        {
            *constraint = zonk(state, context, *constraint)?;
        }
    }
    state.checked.tree.arena.expressions = expressions;

    let mut binders = mem::take(&mut state.checked.tree.arena.binders);
    for (_, binder) in binders.iter_mut() {
        binder.type_id = zonk(state, context, binder.type_id)?;
        if let BinderKind::Typed { annotation, .. } = &mut binder.kind {
            *annotation = zonk(state, context, *annotation)?;
        }
    }
    state.checked.tree.arena.binders = binders;

    let mut local_declarations = mem::take(&mut state.checked.tree.arena.lets);
    for (_, declaration) in local_declarations.iter_mut() {
        declaration.type_id = zonk(state, context, declaration.type_id)?;
        zonk_declaration_abstractions(state, context, &mut declaration.value.abstractions)?;
    }
    state.checked.tree.arena.lets = local_declarations;

    let mut term_declarations = mem::take(&mut state.checked.tree.arena.terms);
    for (_, declaration) in term_declarations.iter_mut() {
        declaration.type_id = zonk(state, context, declaration.type_id)?;
        match &mut declaration.kind {
            TermDeclarationKind::Value(value) => {
                zonk_declaration_abstractions(state, context, &mut value.abstractions)?;
            }
            TermDeclarationKind::Foreign => {}
            TermDeclarationKind::Constructor(constructor) => {
                if let Some(arguments) = zonk_types(state, context, &constructor.arguments)? {
                    constructor.arguments = arguments;
                }
            }
            TermDeclarationKind::Instance(instance) => {
                for parameter in Arc::make_mut(&mut instance.rigid_parameters) {
                    *parameter = zonk(state, context, *parameter)?;
                }
                for evidence in Arc::make_mut(&mut instance.evidences) {
                    evidence.constraint = zonk(state, context, evidence.constraint)?;
                }
                for superclass in Arc::make_mut(&mut instance.superclasses) {
                    superclass.constraint = zonk(state, context, superclass.constraint)?;
                }
                match &mut instance.implementation {
                    InstanceImplementation::Members(members) => {
                        for member in Arc::make_mut(members) {
                            member.implementation_type =
                                zonk(state, context, member.implementation_type)?;
                            zonk_declaration_abstractions(
                                state,
                                context,
                                &mut member.abstractions,
                            )?;
                        }
                    }
                    InstanceImplementation::Delegate { constraint, .. } => {
                        *constraint = zonk(state, context, *constraint)?;
                    }
                }
            }
        }
    }
    state.checked.tree.arena.terms = term_declarations;

    let mut type_declarations = mem::take(&mut state.checked.tree.arena.types);
    for (_, declaration) in type_declarations.iter_mut() {
        declaration.kind = zonk(state, context, declaration.kind)?;
        match &mut declaration.declaration {
            TypeDeclarationKind::Data(_) | TypeDeclarationKind::Newtype(_) => {}
            TypeDeclarationKind::Class(class) => {
                for superclass in Arc::make_mut(&mut class.superclasses) {
                    superclass.constraint = zonk(state, context, superclass.constraint)?;
                }

                for member in Arc::make_mut(&mut class.members) {
                    member.field_type = zonk(state, context, member.field_type)?;
                }
            }
        }
    }
    state.checked.tree.arena.types = type_declarations;

    Ok(())
}

pub fn zonk_evidence<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let binders = state.checked.evidence.binders().map(|(id, binder)| (id, binder.constraint));
    let binders = binders.collect::<Vec<_>>();
    for (id, constraint) in binders {
        let constraint = zonk(state, context, constraint)?;
        state.checked.evidence.bind_binder(id, constraint);
    }

    Ok(())
}

pub fn zonk_holes<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (source_term, hole) in mem::take(&mut state.checked.holes.terms) {
        let type_id = zonk(state, context, hole.type_id)?;
        let bindings = zonk_hole_bindings(state, context, &hole.bindings)?;
        let bindings = holes::refine_bindings(state, context, type_id, bindings)?;

        let hole = TermHole { type_id, bindings };
        state.checked.holes.terms.insert(source_term, hole);
    }

    for (source_type, hole) in mem::take(&mut state.checked.holes.types) {
        let type_id = zonk(state, context, hole.type_id)?;
        let kind_id = zonk(state, context, hole.kind_id)?;
        let bindings = zonk_hole_bindings(state, context, &hole.bindings)?;
        let bindings = holes::refine_bindings(state, context, kind_id, bindings)?;

        let hole = TypeHole { type_id, kind_id, bindings };
        state.checked.holes.types.insert(source_type, hole);
    }

    Ok(())
}

pub fn zonk_errors<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for CheckingError { kind, crumbs } in mem::take(&mut state.checked.errors) {
        let kind = zonk_error_kind(state, context, kind)?;
        state.checked.errors.push(CheckingError { kind, crumbs });
    }

    Ok(())
}

fn zonk_error_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: ErrorKind,
) -> QueryResult<ErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        ErrorKind::Constraint(kind) => zonk_constraint_error(state, context, kind)?.into(),
        ErrorKind::Capability(kind) => zonk_capability_error(state, context, kind)?.into(),
        ErrorKind::CapabilityBundle(kind) => {
            zonk_capability_bundle_error(state, context, kind)?.into()
        }
        ErrorKind::Instance(kind) => zonk_instance_error(state, context, kind)?.into(),
        ErrorKind::Derive(kind) => zonk_derive_error(state, context, kind)?.into(),
        ErrorKind::Unification(kind) => zonk_unification_error(state, context, kind)?.into(),
        ErrorKind::Type(kind) => zonk_type_error(state, context, kind)?.into(),
        kind @ (ErrorKind::Term(_) | ErrorKind::Hole(_)) => kind,
    })
}

fn zonk_constraint_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: ConstraintErrorKind,
) -> QueryResult<ConstraintErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        ConstraintErrorKind::AmbiguousConstraint { constraint } => {
            let constraint = zonk(state, context, constraint)?;
            ConstraintErrorKind::AmbiguousConstraint { constraint }
        }
        ConstraintErrorKind::NoInstanceFound { given, constraint } => {
            let constraint = zonk(state, context, constraint)?;
            let given = zonk_types(state, context, &given)?.unwrap_or(given);
            ConstraintErrorKind::NoInstanceFound { given, constraint }
        }
        ConstraintErrorKind::OverlappingInstances { constraint, instances } => {
            let constraint = zonk(state, context, constraint)?;
            let instances = zonk_types(state, context, &instances)?.unwrap_or(instances);
            ConstraintErrorKind::OverlappingInstances { constraint, instances }
        }
        kind @ (ConstraintErrorKind::CoercibleConstructorNotInScope { .. }
        | ConstraintErrorKind::CustomWarning { .. }
        | ConstraintErrorKind::CustomFailure { .. }
        | ConstraintErrorKind::ReflectedIntegerOutOfRange { .. }) => kind,
    })
}

fn zonk_capability_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: CapabilityErrorKind,
) -> QueryResult<CapabilityErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        CapabilityErrorKind::AmbiguousCapability { service, services, roles } => {
            let service = zonk(state, context, service)?;
            let services = zonk(state, context, services)?;
            CapabilityErrorKind::AmbiguousCapability { service, services, roles }
        }
        CapabilityErrorKind::CapabilityNotDeclared { service, services } => {
            let service = zonk(state, context, service)?;
            let services = zonk(state, context, services)?;
            CapabilityErrorKind::CapabilityNotDeclared { service, services }
        }
        CapabilityErrorKind::UnresolvedCapability { service, services } => {
            let service = zonk(state, context, service)?;
            let services = zonk(state, context, services)?;
            CapabilityErrorKind::UnresolvedCapability { service, services }
        }
    })
}

fn zonk_capability_bundle_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: CapabilityBundleErrorKind,
) -> QueryResult<CapabilityBundleErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        CapabilityBundleErrorKind::InvalidCapabilityBundleMember { member } => {
            let member = zonk(state, context, member)?;
            CapabilityBundleErrorKind::InvalidCapabilityBundleMember { member }
        }
        CapabilityBundleErrorKind::InvalidCapabilityBundlePosition { bundle } => {
            let bundle = zonk(state, context, bundle)?;
            CapabilityBundleErrorKind::InvalidCapabilityBundlePosition { bundle }
        }
    })
}

fn zonk_instance_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: InstanceErrorKind,
) -> QueryResult<InstanceErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        InstanceErrorKind::InstanceHeadLabeledRow { class_file, class_item, position, type_id } => {
            let type_id = zonk(state, context, type_id)?;
            InstanceErrorKind::InstanceHeadLabeledRow { class_file, class_item, position, type_id }
        }
        InstanceErrorKind::InstanceMemberTypeMismatch { expected, actual } => {
            let expected = zonk(state, context, expected)?;
            let actual = zonk(state, context, actual)?;
            InstanceErrorKind::InstanceMemberTypeMismatch { expected, actual }
        }
        InstanceErrorKind::OrphanInstance { constraint } => {
            let constraint = zonk(state, context, constraint)?;
            InstanceErrorKind::OrphanInstance { constraint }
        }
        kind @ (InstanceErrorKind::InstanceHeadMismatch { .. }
        | InstanceErrorKind::MissingInstanceMembers { .. }) => kind,
    })
}

fn zonk_derive_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: DeriveErrorKind,
) -> QueryResult<DeriveErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        DeriveErrorKind::CannotDeriveForType { type_id } => {
            let type_id = zonk(state, context, type_id)?;
            DeriveErrorKind::CannotDeriveForType { type_id }
        }
        DeriveErrorKind::ContravariantOccurrence { type_id } => {
            let type_id = zonk(state, context, type_id)?;
            DeriveErrorKind::ContravariantOccurrence { type_id }
        }
        DeriveErrorKind::CovariantOccurrence { type_id } => {
            let type_id = zonk(state, context, type_id)?;
            DeriveErrorKind::CovariantOccurrence { type_id }
        }
        DeriveErrorKind::ExpectedNewtype { type_id } => {
            let type_id = zonk(state, context, type_id)?;
            DeriveErrorKind::ExpectedNewtype { type_id }
        }
        DeriveErrorKind::NonLocalNewtype { type_id } => {
            let type_id = zonk(state, context, type_id)?;
            DeriveErrorKind::NonLocalNewtype { type_id }
        }
        kind @ (DeriveErrorKind::CannotDeriveClass { .. }
        | DeriveErrorKind::DeriveInvalidArity { .. }
        | DeriveErrorKind::DeriveMissingClass { .. }
        | DeriveErrorKind::InvalidNewtypeDeriveSkolemArguments) => kind,
    })
}

fn zonk_unification_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: UnificationErrorKind,
) -> QueryResult<UnificationErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        UnificationErrorKind::CannotUnify { t1, t2, outer: (outer_t1, outer_t2) } => {
            let t1 = zonk(state, context, t1)?;
            let t2 = zonk(state, context, t2)?;
            let outer_t1 = zonk(state, context, outer_t1)?;
            let outer_t2 = zonk(state, context, outer_t2)?;
            UnificationErrorKind::CannotUnify { t1, t2, outer: (outer_t1, outer_t2) }
        }
        UnificationErrorKind::EscapedSkolem { skolem, type_id } => {
            let skolem = zonk(state, context, skolem)?;
            let type_id = zonk(state, context, type_id)?;
            UnificationErrorKind::EscapedSkolem { skolem, type_id }
        }
        UnificationErrorKind::CannotGeneraliseRecursiveFunction { type_id } => {
            let type_id = zonk(state, context, type_id)?;
            UnificationErrorKind::CannotGeneraliseRecursiveFunction { type_id }
        }
        kind @ (UnificationErrorKind::PropertyIsMissing { .. }
        | UnificationErrorKind::AdditionalProperty { .. }
        | UnificationErrorKind::UnreportedUnknownType { .. }) => kind,
    })
}

fn zonk_type_error<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    kind: TypeErrorKind,
) -> QueryResult<TypeErrorKind>
where
    Q: ExternalQueries,
{
    Ok(match kind {
        TypeErrorKind::InvalidTypeApplication { function_type, function_kind, argument_type } => {
            let function_type = zonk(state, context, function_type)?;
            let function_kind = zonk(state, context, function_kind)?;
            let argument_type = zonk(state, context, argument_type)?;
            TypeErrorKind::InvalidTypeApplication { function_type, function_kind, argument_type }
        }
        TypeErrorKind::NoVisibleTypeVariable { function_type } => {
            let function_type = zonk(state, context, function_type)?;
            TypeErrorKind::NoVisibleTypeVariable { function_type }
        }
        kind @ (TypeErrorKind::PartialSynonymApplication { .. }
        | TypeErrorKind::RecursiveSynonymExpansion { .. }
        | TypeErrorKind::TypeSignatureVariableMismatch { .. }
        | TypeErrorKind::InvalidRoleDeclaration { .. }) => kind,
    })
}

fn zonk_hole_bindings<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[HoleBinding],
) -> QueryResult<Vec<HoleBinding>>
where
    Q: ExternalQueries,
{
    let bindings = bindings.iter().map(|binding| {
        let name = SmolStr::clone(&binding.name);
        let type_id = zonk(state, context, binding.type_id)?;
        Ok(HoleBinding { name, type_id })
    });
    bindings.collect()
}

fn zonk_operator_branch<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    branch_types: OperatorBranchTypes,
) -> QueryResult<OperatorBranchTypes>
where
    Q: ExternalQueries,
{
    Ok(OperatorBranchTypes {
        left: zonk(state, context, branch_types.left)?,
        right: zonk(state, context, branch_types.right)?,
        result: zonk(state, context, branch_types.result)?,
    })
}
