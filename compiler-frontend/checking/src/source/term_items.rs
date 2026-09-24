use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTermItemKind, InstanceItemId, InstanceSourceItemId, TermItemId, TypeItemId};
use itertools::Itertools;
use lowering::TermItemKind;
use rustc_hash::FxHashMap;

use crate::context::CheckContext;
use crate::core::constraint::ConstraintInScope;
use crate::core::substitute::{NameToType, RigidRenaming, SubstituteName};
use crate::core::{
    ApplicationArgument, CheckedInstance, Type, TypeId, constraint, exhaustive, generalise,
    normalise, signature, toolkit, unification, zonk,
};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::evidence::{Evidence, SuperclassId};
use crate::source::terms::equations;
use crate::source::{derive, types};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

#[derive(Default)]
struct TermSccState {
    value_groups: FxHashMap<TermItemId, PendingValueGroup>,
}

enum PendingValueGroup {
    Checked {
        residuals: Vec<ConstraintInScope>,
        abstractions: Vec<tree::DeclarationAbstraction>,
        equations: Vec<equations::ElaboratedEquation>,
    },
    Inferred {
        residuals: Vec<ConstraintInScope>,
        equations: Vec<equations::ElaboratedEquation>,
    },
}

pub fn check_term_items<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    check_instance_declarations(state, context)?;
    let derive_results = derive::check_derive_declarations(state, context)?;
    check_overlapping_instance_declarations(state, context)?;
    check_value_groups(state, context)?;
    check_instance_members(state, context)?;
    derive::check_derive_members(state, context, &derive_results)?;
    Ok(())
}

pub fn check_instance_declarations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for &item_id in &context.grouped.instance_items {
        let Some(item) = context.lowered.tree.get_instance_item(item_id) else { continue };
        let resolution = item.resolution;
        let item = CheckInstanceDeclaration {
            item_id,
            constraints: &item.constraints,
            resolution,
            arguments: &item.arguments,
        };
        check_instance_declaration(state, context, item)?;
    }

    Ok(())
}

fn check_overlapping_instance_declarations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut candidate_arguments = FxHashMap::default();
    for &item_id in &context.grouped.instance_sources {
        let crumb = match item_id {
            InstanceSourceItemId::Instance(id) => ErrorCrumb::InstanceDeclaration(id),
            InstanceSourceItemId::Derive(id) => ErrorCrumb::DeriveDeclaration(id),
        };
        state.with_error_crumb(crumb, |state| {
            constraint::instances::validate_declared_instance_overlap(
                state,
                context,
                item_id,
                &mut candidate_arguments,
            )
        })?;
    }

    Ok(())
}

struct CheckInstanceDeclaration<'a> {
    item_id: InstanceItemId,
    constraints: &'a [lowering::TypeId],
    resolution: Option<(FileId, TypeItemId)>,
    arguments: &'a [lowering::TypeId],
}

fn check_instance_declaration<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: CheckInstanceDeclaration,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let CheckInstanceDeclaration { item_id, constraints, resolution, arguments } = item;

    let Some((class_file, class_id)) = resolution else {
        return Ok(());
    };

    let instance_id = context.indexed.items[item_id].id;

    let class_kind = toolkit::lookup_file_type(state, context, class_file, class_id)?;

    let expected_kinds = {
        let signature = signature::decompose_signature(
            state,
            context,
            class_kind,
            signature::DecomposeSignatureMode::Full,
        )?;
        signature.arguments().collect_vec()
    };

    if expected_kinds.len() != arguments.len() {
        state.insert_error(ErrorKind::InstanceHeadMismatch {
            class_file,
            class_item: class_id,
            expected: expected_kinds.len(),
            actual: arguments.len(),
        });
    }

    let mut class_type = context.queries.intern_type(Type::Constructor(class_file, class_id));
    let mut class_kind = class_kind;
    let mut checked_arguments = Vec::with_capacity(arguments.len());

    for &argument in arguments {
        (class_type, class_kind) =
            types::infer_application_kind(state, context, (class_type, class_kind), argument)?;
        let (_, extracted_arguments) =
            toolkit::extract_type_application(state, context, class_type)?;
        if let Some(&checked_argument) = extracted_arguments.last() {
            checked_arguments.push(checked_argument);
        }
    }

    unification::subtype(state, context, class_kind, context.prim.constraint)?;

    let mut checked_constraints = Vec::with_capacity(constraints.len());
    for &constraint in constraints {
        let (constraint_type, _) =
            types::check_kind(state, context, constraint, context.prim.constraint)?;
        checked_constraints.push(constraint_type);
    }

    let mut canonical = class_type;
    for &constraint in checked_constraints.iter().rev() {
        canonical = context.intern_constrained(constraint, canonical);
    }

    constraint::instances::validate_rows(state, context, class_file, class_id, &checked_arguments)?;

    let resolution = (class_file, class_id);
    let canonical = zonk::zonk(state, context, canonical)?;
    let signature = generalise::generalise_implicit(state, context, canonical)?;
    let matchable = toolkit::freshen_instance_signature(state, context, signature)?;

    let instance = CheckedInstance { resolution, signature, matchable };
    state.checked.instances.insert(instance_id, instance);

    Ok(())
}

fn check_value_groups<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for scc in &context.grouped.term_scc {
        let items = scc.as_slice();

        for &item in items {
            check_term_signature(state, context, item)?;
        }

        let recursive = scc.is_recursive();
        if recursive {
            prepare_binding_group(state, context, items);
        }

        let mut term_scc = TermSccState::default();

        for &item in items {
            check_term_equation(state, context, &mut term_scc, item)?;
        }

        finalise_term_binding_group(state, context, &mut term_scc, items, recursive)?;
    }

    Ok(())
}

fn check_instance_members<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for &item_id in &context.grouped.instance_items {
        let Some(item) = context.lowered.tree.get_instance_item(item_id) else { continue };
        let Some((class_file, class_id)) = item.resolution else {
            continue;
        };
        let instance_id = context.indexed.items[item_id].id;

        let Some(checked_instance) = state.checked.lookup_instance(instance_id) else {
            continue;
        };

        let Some(instance) = toolkit::instance_info(
            state,
            context,
            checked_instance.signature,
            checked_instance.resolution,
        )?
        else {
            continue;
        };

        inherit_instance_kind_names(state, context, (class_file, class_id), &instance.arguments)?;

        check_instance_member_groups(
            state,
            context,
            item_id,
            &item.members,
            (class_file, class_id),
            checked_instance.signature,
            &instance,
        )?;
    }

    Ok(())
}

fn inherit_instance_kind_names<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    resolution: (FileId, TypeItemId),
    instance_arguments: &[ApplicationArgument],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(class) = toolkit::lookup_file_class(state, context, resolution.0, resolution.1)?
    else {
        return Ok(());
    };

    let mut arguments = instance_arguments.iter().copied();
    for &binder_id in class.kind_binders.iter() {
        let Some(ApplicationArgument::Kind(argument)) = arguments.next() else {
            break;
        };
        let binder = context.lookup_forall_binder(binder_id);
        let Some(text) = toolkit::lookup_name(state, context, binder.name)? else {
            continue;
        };
        let Type::Rigid(name, _, _) = *context.lookup_type(argument) else {
            continue;
        };
        state.checked.names.entry(name).or_insert(text);
    }

    Ok(())
}

fn check_instance_member_groups<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    instance_item_id: InstanceItemId,
    members: &[lowering::InstanceMemberGroup],
    (class_file, class_id): (FileId, TypeItemId),
    instance_signature: TypeId,
    instance: &toolkit::InstanceInfo,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    state.with_error_crumb(ErrorCrumb::InstanceDeclaration(instance_item_id), |state| {
        state.with_implication(|state| {
            let FreshenedInstanceRigids {
                constraints: instance_constraints,
                arguments: instance_arguments,
                renaming,
                rigids,
            } = freshen_instance_rigids(state, context, instance)?;

            state.with_source_type_renaming(&renaming, |state| {
                debug_assert_eq!(instance_constraints.len(), instance.constraints.len());
                let mut instance_evidences = vec![];
                for (&constraint, &signature_constraint) in
                    std::iter::zip(&instance_constraints, &instance.constraints)
                {
                    let evidence = state.push_given(constraint);
                    let evidence = Evidence::Given(evidence);
                    instance_evidences.push(tree::InstanceEvidence {
                        constraint: signature_constraint,
                        evidence,
                    });
                }

                let superclasses = emit_instance_superclass_constraints(
                    state,
                    context,
                    class_file,
                    class_id,
                    &instance_arguments,
                )?;

                let mut checked_members = vec![];
                for member in members {
                    let checked_member = state.with_implication(|state| {
                        check_instance_member_group(
                            state,
                            context,
                            member,
                            (class_file, class_id),
                            &instance_arguments,
                        )
                    })?;
                    if let Some(checked_member) = checked_member {
                        checked_members.push(checked_member);
                    }
                }

                let mut suppress_missing_member_warning = false;
                if members.is_empty() {
                    for &constraint in &instance_constraints {
                        let Some(canonical) =
                            constraint::canonical::canonicalise(state, context, constraint)?
                        else {
                            continue;
                        };
                        if constraint::compiler::is_fail_constraint(state, context, canonical) {
                            suppress_missing_member_warning = true;
                            break;
                        }
                    }
                }

                if let Some(class) =
                    toolkit::lookup_file_class(state, context, class_file, class_id)?
                {
                    let indexed = context.queries.indexed(class_file)?;
                    let missing_members = class.members.iter().filter_map(|member| {
                        let implemented = checked_members.iter().any(|implementation| {
                            implementation.resolution == (class_file, member.item_id)
                        });
                        if implemented { None } else { indexed.items[member.item_id].name.clone() }
                    });
                    let missing_members = missing_members.collect::<Arc<[_]>>();
                    if !missing_members.is_empty() && !suppress_missing_member_warning {
                        state.insert_error(ErrorKind::MissingInstanceMembers {
                            members: missing_members,
                        });
                    }

                    let positions = class
                        .members
                        .iter()
                        .enumerate()
                        .map(|(position, member)| (member.item_id, position));
                    let positions = positions.collect::<FxHashMap<_, _>>();
                    checked_members.sort_by_key(|member| {
                        if member.resolution.0 == class_file {
                            positions.get(&member.resolution.1).copied().unwrap_or(usize::MAX)
                        } else {
                            usize::MAX
                        }
                    });
                }

                derive::tools::solve_and_report_constraints(state, context)?;

                let instance = tree::InstanceDeclaration {
                    class: (class_file, class_id),
                    rigid_parameters: Arc::from(rigids),
                    evidences: Arc::from(instance_evidences),
                    superclasses: Arc::from(superclasses),
                    implementation: tree::InstanceImplementation::Members(Arc::from(
                        checked_members,
                    )),
                };
                let declaration = tree::TermDeclaration {
                    type_id: instance_signature,
                    kind: tree::TermDeclarationKind::Instance(instance),
                };
                state.checked.tree.insert_instance(instance_item_id, declaration);
                Ok(())
            })
        })
    })
}

fn check_instance_member_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    member: &lowering::InstanceMemberGroup,
    (class_file, class_id): (FileId, TypeItemId),
    instance_arguments: &[ApplicationArgument],
) -> QueryResult<Option<tree::InstanceMember>>
where
    Q: ExternalQueries,
{
    let class_member_type = if let Some(member_resolution) = member.resolution {
        instantiate_class_member_type(
            state,
            context,
            member_resolution,
            (class_file, class_id),
            instance_arguments,
        )?
    } else {
        None
    };

    if let Some(signature_id) = member.signature {
        let (signature_member_type, _) =
            types::check_kind(state, context, signature_id, context.prim.t)?;

        if let Some(class_member_type) = class_member_type {
            let unified = state.with_implication(|state| {
                let class_member_type = normalise::normalise(state, context, class_member_type);
                let class_member_type =
                    toolkit::skolemise_forall(state, context, class_member_type)?;
                let class_member_type = toolkit::collect_givens(state, context, class_member_type)?;
                unification::subtype(state, context, signature_member_type, class_member_type)
            })?;
            if !unified {
                let expected = class_member_type;
                let actual = signature_member_type;
                state.insert_error(ErrorKind::InstanceMemberTypeMismatch { expected, actual });
            }
        }

        let checked_equations = equations::check_value_equations(
            state,
            context,
            equations::EquationTypeOrigin::Explicit(signature_id),
            signature_member_type,
            &member.equations,
        )?;
        let exhaustiveness = exhaustive::check_equation_patterns(
            state,
            context,
            &checked_equations.patterns,
            &member.equations,
        )?;

        state.report_exhaustiveness(exhaustiveness);
        Ok(record_instance_member(member, signature_member_type, checked_equations))
    } else if let Some(expected_type) = class_member_type {
        let checked_equations = equations::check_value_equations(
            state,
            context,
            equations::EquationTypeOrigin::Implicit,
            expected_type,
            &member.equations,
        )?;
        let exhaustiveness = exhaustive::check_equation_patterns(
            state,
            context,
            &checked_equations.patterns,
            &member.equations,
        )?;

        state.report_exhaustiveness(exhaustiveness);
        Ok(record_instance_member(member, expected_type, checked_equations))
    } else {
        Ok(None)
    }
}

fn record_instance_member(
    member: &lowering::InstanceMemberGroup,
    implementation_type: TypeId,
    checked: equations::CheckedValueEquations,
) -> Option<tree::InstanceMember> {
    let resolution = member.resolution?;
    let equations = checked.equations.into_iter().map(equations::ElaboratedEquation::into_tree);
    let equations = equations.collect::<Option<Vec<_>>>()?;

    let complete = !member.equations.is_empty()
        && member.equations.len() == equations.len()
        && std::iter::zip(member.equations.iter(), &equations).all(|(source, equation)| {
            source.source.map(tree::EquationSource::Item) == Some(equation.source)
        });
    if !complete {
        return None;
    }

    Some(tree::InstanceMember {
        resolution,
        implementation_type,
        abstractions: Arc::from(checked.abstractions),
        equations: Arc::from(equations),
    })
}

pub(crate) struct FreshenedInstanceRigids {
    pub(crate) constraints: Vec<TypeId>,
    pub(crate) arguments: Vec<ApplicationArgument>,
    pub(crate) renaming: Arc<RigidRenaming>,
    pub(crate) rigids: Vec<TypeId>,
}

pub(crate) fn freshen_instance_rigids<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    instance: &toolkit::InstanceInfo,
) -> QueryResult<FreshenedInstanceRigids>
where
    Q: ExternalQueries,
{
    let mut renaming = RigidRenaming::default();
    let mut rigids = Vec::with_capacity(instance.binders.len());

    for binder in &instance.binders {
        let kind = renaming.substitute(state, context, binder.kind)?;
        let text = toolkit::lookup_name(state, context, binder.name)?;
        let rigid = state.fresh_rigid_named(context.queries, kind, text);
        renaming.insert(context, binder.name, rigid);
        rigids.push(rigid);
    }

    let constraints = instance
        .constraints
        .iter()
        .map(|&constraint| renaming.substitute(state, context, constraint))
        .collect::<QueryResult<Vec<_>>>()?;

    let arguments = instance
        .arguments
        .iter()
        .map(|&argument| substitute_kind_or_type(state, context, &renaming, argument))
        .collect::<QueryResult<Vec<_>>>()?;

    let renaming = Arc::new(renaming);
    Ok(FreshenedInstanceRigids { constraints, arguments, renaming, rigids })
}

pub(crate) fn emit_instance_superclass_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    instance_arguments: &[ApplicationArgument],
) -> QueryResult<Vec<tree::InstanceSuperclass>>
where
    Q: ExternalQueries,
{
    let Some(class) = toolkit::lookup_file_class(state, context, class_file, class_id)? else {
        return Ok(vec![]);
    };
    let Some(substitution) =
        constraint::elaborate::superclass_substitutions(context, &class, instance_arguments)?
    else {
        return Ok(vec![]);
    };

    let mut checked_superclasses = Vec::with_capacity(class.superclasses.len());
    for superclass in &class.superclasses {
        let constraint =
            SubstituteName::many(state, context, &substitution, superclass.constraint)?;
        let evidence = state.push_wanted(constraint);
        let id = SuperclassId {
            file_id: class_file,
            type_id: class_id,
            source_id: superclass.source_id,
        };
        checked_superclasses.push(tree::InstanceSuperclass { id, constraint, evidence });
    }

    Ok(checked_superclasses)
}

pub(crate) fn instantiate_class_member_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (member_file, member_id): (FileId, TermItemId),
    (class_file, class_id): (FileId, TypeItemId),
    instance_arguments: &[ApplicationArgument],
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    let Some(class_info) = toolkit::lookup_file_class(state, context, class_file, class_id)? else {
        return Ok(None);
    };

    if member_file != class_file {
        return Ok(None);
    }
    let Some(member) = class_info.members.iter().find(|member| member.item_id == member_id) else {
        return Ok(None);
    };

    let mut bindings = NameToType::default();
    let mut instance_arguments = instance_arguments.iter().copied();

    for &binder_id in class_info.kind_binders.iter() {
        let Some(ApplicationArgument::Kind(argument)) = instance_arguments.next() else {
            return Ok(None);
        };
        let binder = context.lookup_forall_binder(binder_id);
        bindings.insert(binder.name, argument);
    }

    for &binder_id in class_info.type_parameters.iter() {
        let Some(ApplicationArgument::Type(argument)) = instance_arguments.next() else {
            return Ok(None);
        };
        let binder = context.lookup_forall_binder(binder_id);
        bindings.insert(binder.name, argument);
    }

    if instance_arguments.next().is_some() {
        return Ok(None);
    }

    let field_type = SubstituteName::many(state, context, &bindings, member.field_type)?;
    let field_type = normalise::normalise(state, context, field_type);
    Ok(Some(field_type))
}

fn substitute_kind_or_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    renaming: &RigidRenaming,
    argument: ApplicationArgument,
) -> QueryResult<ApplicationArgument>
where
    Q: ExternalQueries,
{
    Ok(match argument {
        ApplicationArgument::Kind(argument) => {
            ApplicationArgument::Kind(renaming.substitute(state, context, argument)?)
        }
        ApplicationArgument::Type(argument) => {
            ApplicationArgument::Type(renaming.substitute(state, context, argument)?)
        }
    })
}

fn prepare_binding_group<Q>(state: &mut CheckState, context: &CheckContext<Q>, items: &[TermItemId])
where
    Q: ExternalQueries,
{
    for &item_id in items {
        if state.checked.term_item_types.contains_key(&item_id) {
            continue;
        }

        let item = context.lowered.tree.get_term_item_kind(item_id);

        let resolution = item.and_then(|item| match item {
            TermItemKind::Operator { resolution, .. } => *resolution,
            _ => None,
        });

        let item_type = resolution.and_then(|(file_id, item_id)| {
            if file_id == context.id { state.checked.lookup_term_item_type(item_id) } else { None }
        });

        let item_type = if let Some(item_type) = item_type {
            item_type
        } else {
            state.fresh_unification(context.queries, context.prim.t)
        };

        state.checked.term_item_types.insert(item_id, item_type);
    }
}

fn check_term_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(item) = context.lowered.tree.get_term_item_kind(item_id) else {
        return Ok(());
    };

    match item {
        TermItemKind::Foreign { signature } => {
            let Some(signature) = signature else { return Ok(()) };
            let type_id = check_signature_type(state, context, item_id, *signature)?;
            let declaration =
                tree::TermDeclaration { type_id, kind: tree::TermDeclarationKind::Foreign };
            state.checked.tree.insert_term(item_id, declaration);
        }
        TermItemKind::Value { signature, .. } => {
            let Some(signature) = signature else { return Ok(()) };
            check_signature_type(state, context, item_id, *signature)?;
        }
        _ => (),
    }

    Ok(())
}

fn check_signature_type<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
    signature: lowering::TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    let (checked_kind, _) = types::check_kind(state, context, signature, context.prim.t)?;
    state.checked.term_item_types.insert(item_id, checked_kind);
    Ok(checked_kind)
}

fn check_term_equation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TermSccState,
    item_id: TermItemId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(item) = context.lowered.tree.get_term_item_kind(item_id) else {
        return Ok(());
    };

    match item {
        TermItemKind::Operator { resolution, .. } => {
            check_term_operator(state, context, item_id, *resolution)?;
        }
        TermItemKind::Value { signature, equations } => {
            let pending = state.with_implication(|state| {
                check_value_group(state, context, item_id, *signature, equations)
            })?;
            if let Some(pending) = pending {
                scc.value_groups.insert(item_id, pending);
            }
        }
        _ => (),
    }

    Ok(())
}

fn check_value_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
    signature: Option<lowering::TypeId>,
    equations: &[lowering::Equation],
) -> QueryResult<Option<PendingValueGroup>>
where
    Q: ExternalQueries,
{
    state.with_error_crumb(ErrorCrumb::TermDeclaration(item_id), |state| {
        check_value_group_core(state, context, item_id, signature, equations)
    })
}

fn check_value_group_core<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
    signature: Option<lowering::TypeId>,
    equations: &[lowering::Equation],
) -> QueryResult<Option<PendingValueGroup>>
where
    Q: ExternalQueries,
{
    if let Some(signature_id) = signature
        && let Some(signature_type) = state.checked.lookup_term_item_type(item_id)
    {
        let (residuals, abstractions, equations) =
            check_value_group_core_check(state, context, signature_id, signature_type, equations)?;
        Ok(Some(PendingValueGroup::Checked { residuals, abstractions, equations }))
    } else {
        let (residuals, equations) =
            check_value_group_core_infer(state, context, item_id, equations)?;
        Ok(Some(PendingValueGroup::Inferred { residuals, equations }))
    }
}

fn check_value_group_core_check<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature_id: lowering::TypeId,
    signature_type: TypeId,
    equations: &[lowering::Equation],
) -> QueryResult<(
    Vec<ConstraintInScope>,
    Vec<tree::DeclarationAbstraction>,
    Vec<equations::ElaboratedEquation>,
)>
where
    Q: ExternalQueries,
{
    let checked_equations = equations::check_value_equations(
        state,
        context,
        equations::EquationTypeOrigin::Explicit(signature_id),
        signature_type,
        equations,
    )?;
    let exhaustiveness = exhaustive::check_equation_patterns(
        state,
        context,
        &checked_equations.patterns,
        equations,
    )?;
    state.report_exhaustiveness(exhaustiveness);
    let residuals = state.solve_constraints(context)?;
    Ok((residuals, checked_equations.abstractions, checked_equations.equations))
}

fn check_value_group_core_infer<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
    equations: &[lowering::Equation],
) -> QueryResult<(Vec<ConstraintInScope>, Vec<equations::ElaboratedEquation>)>
where
    Q: ExternalQueries,
{
    let group_type = state.fresh_unification(context.queries, context.prim.t);
    state.checked.term_item_types.insert(item_id, group_type);
    let checked_equations =
        equations::infer_value_equations(state, context, group_type, equations)?;
    let exhaustiveness = exhaustive::check_equation_patterns(
        state,
        context,
        &checked_equations.patterns,
        equations,
    )?;
    let has_missing = exhaustiveness.missing.is_some();
    state.report_exhaustiveness(exhaustiveness);
    if has_missing {
        state.push_wanted(context.prim.partial);
    }

    let residuals = state.solve_constraints(context)?;
    Ok((residuals, checked_equations.equations))
}

fn finalise_term_binding_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TermSccState,
    items: &[TermItemId],
    recursive: bool,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    struct Pending {
        marker: TypeId,
        unsolved: Vec<u32>,
        errors: generalise::ConstraintErrors,
        abstractions: Vec<tree::DeclarationAbstraction>,
        equations: Vec<equations::ElaboratedEquation>,
        inferred_constraints: bool,
    }

    let mut pending = vec![];

    for &item_id in items {
        let Some(marker) = state.checked.term_item_types.get(&item_id).copied() else {
            continue;
        };

        let group = scc.value_groups.remove(&item_id);
        let marker = zonk::zonk(state, context, marker)?;

        let mut errors = generalise::ConstraintErrors::default();

        let (marker, abstractions, equations, inferred_constraints) = match group {
            Some(PendingValueGroup::Checked { residuals, abstractions, equations }) => {
                errors.unsatisfied.extend(residuals);
                (marker, abstractions, equations, false)
            }
            Some(PendingValueGroup::Inferred { residuals, equations }) => {
                let constrained = generalise::constrain_using_residuals(
                    state,
                    context,
                    marker,
                    residuals,
                    &mut errors,
                )?;
                let inferred_constraints = !constrained.constraints.is_empty();
                let abstractions = constrained.constraints.into_iter().map(|constraint| {
                    tree::DeclarationAbstraction::Evidence {
                        constraint: constraint.constraint,
                        binder: constraint.binder,
                    }
                });
                let abstractions = abstractions.collect();
                (constrained.type_id, abstractions, equations, inferred_constraints)
            }
            None => (marker, vec![], vec![], false),
        };

        let marker = zonk::zonk(state, context, marker)?;
        let unsolved = generalise::unsolved_unifications(state, context, marker)?;

        let pending_group =
            Pending { marker, unsolved, errors, abstractions, equations, inferred_constraints };
        pending.push((item_id, pending_group));
    }

    for (
        item_id,
        Pending { marker, unsolved, errors, abstractions, equations, inferred_constraints },
    ) in pending
    {
        let generalised =
            generalise::generalise_unsolved_with_parameters(state, context, marker, &unsolved)?;
        let marker = generalised.type_id;
        state.checked.term_item_types.insert(item_id, marker);

        let type_abstractions = generalised.parameters.into_iter().map(|parameter| {
            tree::DeclarationAbstraction::Type { binder: parameter.binder, rigid: parameter.rigid }
        });
        let abstractions = type_abstractions.chain(abstractions);
        let abstractions = abstractions.collect();

        if recursive && inferred_constraints {
            // Keep constraint evidence consistent with the candidate type used for error recovery.
            // The diagnostic prevents the invalid recursive dictionary abstraction from proceeding.
            state.with_error_crumb(ErrorCrumb::TermDeclaration(item_id), |state| {
                let error = ErrorKind::CannotGeneraliseRecursiveFunction { type_id: marker };
                state.insert_error(error);
            });
        }

        record_value_declaration(state, context, item_id, marker, abstractions, equations);

        for error in errors.ambiguous {
            let constraint = state.canonicals.type_id(context, error);
            state.with_error_crumb(ErrorCrumb::TermDeclaration(item_id), |state| {
                state.insert_error(ErrorKind::AmbiguousConstraint { constraint });
            });
        }
        for error in errors.unsatisfied {
            state.checked.evidence.mark_error(error.evidence.wanted);
            state.with_error_crumb(ErrorCrumb::TermDeclaration(item_id), |state| {
                let attached = state.canonical_errors.remove(&error.key.wanted);
                attached.into_iter().flatten().for_each(|error| state.insert_error(error));
            });

            let given = error
                .key
                .given
                .iter()
                .map(|given| state.canonicals.type_id(context, *given))
                .collect::<Arc<[_]>>();

            let constraint = state.canonicals.type_id(context, error.key.wanted);
            state.with_error_crumb(ErrorCrumb::TermDeclaration(item_id), |state| {
                state.insert_error(ErrorKind::NoInstanceFound { given, constraint });
            });
        }
    }

    Ok(())
}

fn record_value_declaration<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
    type_id: TypeId,
    abstractions: Vec<tree::DeclarationAbstraction>,
    equations: Vec<equations::ElaboratedEquation>,
) where
    Q: ExternalQueries,
{
    let IndexedTermItemKind::Value { equations: sources, .. } =
        &context.indexed.items[item_id].kind
    else {
        return;
    };
    let equations = equations.into_iter().map(equations::ElaboratedEquation::into_tree);
    let Some(equations) = equations.collect::<Option<Vec<_>>>() else {
        return;
    };

    let complete = !sources.is_empty()
        && sources.len() == equations.len()
        && std::iter::zip(sources, &equations).all(|(source, equation)| {
            let source = indexing::EquationSourceId::Value(*source);
            equation.source == tree::EquationSource::Item(source)
        });
    if !complete {
        return;
    }

    let declaration = tree::ValueDeclaration {
        abstractions: Arc::from(abstractions),
        equations: Arc::from(equations),
    };
    let declaration =
        tree::TermDeclaration { type_id, kind: tree::TermDeclarationKind::Value(declaration) };
    state.checked.tree.insert_term(item_id, declaration);
}

fn check_term_operator<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TermItemId,
    resolution: Option<(FileId, TermItemId)>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some((file_id, term_id)) = resolution else { return Ok(()) };
    let operator_type = toolkit::lookup_file_term_operator(state, context, file_id, term_id)?;

    if let Some(item_type) = state.checked.lookup_term_item_type(item_id) {
        unification::subtype(state, context, operator_type, item_type)?;
    } else {
        state.checked.term_item_types.insert(item_id, operator_type);
    }

    Ok(())
}
