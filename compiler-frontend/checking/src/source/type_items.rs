use std::mem;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTypeItemKind, TermItemId, TypeItemId};
use itertools::Itertools;
use lowering::{
    ClassDeclaration, DataDeclaration, LoweringError, NewtypeDeclaration, RecursiveGroup, Scc,
    TermItemKind, TypeItemKind, TypeSynonymDeclaration, TypeVariableBinding,
};
use smol_str::SmolStr;

use crate::context::CheckContext;
use crate::core::{
    CheckedClass, CheckedClassMember, CheckedDataDeclaration, CheckedSuperclass, CheckedSynonym,
    ForallBinder, Role, Type, TypeId, fd, fold, generalise, signature, toolkit, unification, zonk,
};
use crate::error::ErrorCrumb;
use crate::evidence::SuperclassId;
use crate::source::types;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop, tree};

struct PendingDataType {
    parameters: Vec<ForallBinder>,
    constructors: Vec<(TermItemId, Vec<TypeId>)>,
    declared_roles: Arc<[lowering::Role]>,
}

struct PendingSynonymType {
    parameters: Vec<ForallBinder>,
    synonym: TypeId,
}

struct PendingClassType {
    parameters: Vec<ForallBinder>,
    superclasses: Vec<CheckedSuperclass>,
    functional_dependencies: Arc<[fd::Fd]>,
    members: Vec<(TermItemId, TypeId)>,
}

#[derive(Default)]
struct TypeSccState {
    data: Vec<(TypeItemId, PendingDataType)>,
    synonym: Vec<(TypeItemId, PendingSynonymType)>,
    class: Vec<(TypeItemId, PendingClassType)>,
    foreign: Vec<(TypeItemId, Arc<[lowering::Role]>)>,
}

struct ApplyKinds {
    reference: (FileId, TypeItemId),
    replacement: TypeId,
}

impl ApplyKinds {
    fn on<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        item_id: TypeItemId,
        replacement: TypeId,
        argument: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        let mut folder = ApplyKinds { reference: (context.id, item_id), replacement };
        fold::fold_type(state, context, argument, &mut folder)
    }

    fn has_constructor<Q>(&self, context: &CheckContext<Q>, mut function: TypeId) -> bool
    where
        Q: ExternalQueries,
    {
        safe_loop! {
            match *context.lookup_type(function) {
                Type::KindApplication(inner_function, _) => function = inner_function,
                Type::Constructor(file_id, item_id) => return self.reference == (file_id, item_id),
                _ => return false,
            }
        }
    }
}

impl fold::TypeFold for ApplyKinds {
    fn transform<Q: ExternalQueries>(
        &mut self,
        _state: &mut CheckState,
        context: &CheckContext<Q>,
        id: TypeId,
        t: &Type,
    ) -> QueryResult<fold::FoldAction> {
        if let Type::KindApplication(function, _) = t
            && self.has_constructor(context, *function)
        {
            return Ok(fold::FoldAction::Replace(id));
        }

        if let Type::Constructor(file_id, item_id) = t
            && self.reference == (*file_id, *item_id)
        {
            Ok(fold::FoldAction::Replace(self.replacement))
        } else {
            Ok(fold::FoldAction::Continue)
        }
    }
}

/// Checks all type items in topological order.
///
/// The order is determined by [`GroupedModule::type_scc`] in [`lowering`].
/// The algorithm accounts for items that appear in [`RecursiveKinds`] by
/// filtering and populating these items with [`Type::Unknown`]. This
/// enables checking of other binding groups, which may be unaffected.
///
/// [`GroupedModule::type_scc`]: lowering::GroupedModule::type_scc
/// [`RecursiveKinds`]: LoweringError::RecursiveKinds
/// [`Type::Unknown`]: crate::core::Type::Unknown
pub fn check_type_items<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for scc in &context.grouped.type_scc {
        let (items, skipped) = partition_type_items(context, scc);
        populate_skipped_items(state, context, &skipped);

        for &item in &items {
            check_type_signature(state, context, item)?;
        }

        if scc.is_recursive() {
            prepare_binding_group(state, context, &items);
        }

        let mut scc_state = TypeSccState::default();

        for &item in &items {
            check_type_equation(state, context, &mut scc_state, item)?;
        }

        finalise_type_binding_group(state, context, &items)?;
        finalise_roles(state, context, &mut scc_state)?;
        finalise_data_declarations(state, context, &mut scc_state)?;
        finalise_synonym_replacements(state, context, &mut scc_state)?;
        finalise_classes(state, context, &mut scc_state)?;
    }
    Ok(())
}

fn prepare_binding_group<Q>(state: &mut CheckState, context: &CheckContext<Q>, items: &[TypeItemId])
where
    Q: ExternalQueries,
{
    for &item_id in items {
        if state.checked.type_item_kinds.contains_key(&item_id) {
            continue;
        }
        let kind = state.fresh_unification(context.queries, context.prim.t);
        state.checked.type_item_kinds.insert(item_id, kind);
    }
}

fn finalise_type_binding_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TypeItemId],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut pending = Vec::with_capacity(items.len());

    for &item_id in items {
        let Some(kind) = state.checked.type_item_kinds.get(&item_id).copied() else {
            continue;
        };

        let kind = zonk::zonk(state, context, kind)?;
        let unsolved = generalise::unsolved_unifications(state, context, kind)?;

        pending.push((item_id, kind, unsolved));
    }

    for (item_id, kind, unsolved) in pending {
        let kind = generalise::generalise_unsolved(state, context, kind, &unsolved)?;
        state.checked.type_item_kinds.insert(item_id, kind);
    }

    Ok(())
}

fn partition_type_items<Q>(
    context: &CheckContext<Q>,
    scc: &Scc<TypeItemId>,
) -> (Vec<TypeItemId>, Vec<TypeItemId>)
where
    Q: ExternalQueries,
{
    let mut checked = vec![];
    let mut skipped = vec![];

    for &item_id in scc.as_slice() {
        if is_recursive_kind(context, item_id) {
            skipped.push(item_id);
        } else {
            checked.push(item_id);
        }
    }

    (checked, skipped)
}

fn is_recursive_kind<Q>(context: &CheckContext<Q>, item_id: TypeItemId) -> bool
where
    Q: ExternalQueries,
{
    context.grouped.cycle_errors.iter().any(|error| {
        let LoweringError::RecursiveKinds(RecursiveGroup { group }) = error else {
            return false;
        };
        group.contains(&item_id)
    })
}

fn populate_skipped_items<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TypeItemId],
) where
    Q: ExternalQueries,
{
    let unknown = context.unknown("invalid recursive type");
    let skipped = items.iter().map(|item| (*item, unknown));
    state.checked.type_item_kinds.extend(skipped);
}

fn check_type_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(item) = context.lowered.tree.get_type_item_kind(item_id) else {
        return Ok(());
    };

    match item {
        TypeItemKind::Data { signature, .. } => {
            let Some(signature) = signature else { return Ok(()) };
            check_signature_kind(state, context, item_id, *signature)?;
        }
        TypeItemKind::Newtype { signature, .. } => {
            let Some(signature) = signature else { return Ok(()) };
            check_signature_kind(state, context, item_id, *signature)?;
        }
        TypeItemKind::Synonym { signature, .. } => {
            let Some(signature) = signature else { return Ok(()) };
            check_signature_kind(state, context, item_id, *signature)?;
        }
        TypeItemKind::Class { signature, .. } => {
            let Some(signature) = signature else { return Ok(()) };
            check_signature_kind(state, context, item_id, *signature)?;
        }
        TypeItemKind::Foreign { signature, .. } => {
            let Some(signature) = signature else { return Ok(()) };
            check_signature_kind(state, context, item_id, *signature)?;
        }
        TypeItemKind::Operator { .. } => {}
    }

    Ok(())
}

fn check_signature_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
    signature: lowering::TypeId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let (checked_kind, _) = types::check_kind(state, context, signature, context.prim.t)?;
    state.checked.type_item_kinds.insert(item_id, checked_kind);
    Ok(())
}

fn check_type_equation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
    item_id: TypeItemId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(item) = context.lowered.tree.get_type_item_kind(item_id) else {
        return Ok(());
    };

    match item {
        TypeItemKind::Data { signature, declaration, roles } => {
            let Some(DataDeclaration { variables }) = declaration else { return Ok(()) };
            check_data_equation(state, context, scc, item_id, *signature, variables, roles)?;
        }
        TypeItemKind::Newtype { signature, declaration, roles } => {
            let Some(NewtypeDeclaration { variables }) = declaration else { return Ok(()) };
            check_data_equation(state, context, scc, item_id, *signature, variables, roles)?;
        }
        TypeItemKind::Synonym { signature, declaration, .. } => {
            let Some(TypeSynonymDeclaration { variables, type_ }) = declaration else {
                return Ok(());
            };
            check_synonym_equation(state, context, scc, item_id, *signature, variables, *type_)?;
        }
        TypeItemKind::Class { signature, declaration } => {
            let Some(declaration) = declaration else { return Ok(()) };
            check_class_equation(state, context, scc, item_id, *signature, declaration)?;
        }
        TypeItemKind::Foreign { roles, .. } => {
            scc.foreign.push((item_id, Arc::clone(roles)));
        }
        TypeItemKind::Operator { resolution, .. } => {
            check_type_operator(state, context, item_id, *resolution)?;
        }
    }

    Ok(())
}

fn check_data_equation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
    item_id: TypeItemId,
    signature: Option<lowering::TypeId>,
    variables: &[TypeVariableBinding],
    declared_roles: &Arc<[lowering::Role]>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let parameters = if let Some(signature_id) = signature
        && let Some(signature_kind) = state.checked.lookup_type_item_kind(item_id)
    {
        check_data_equation_check(state, context, (signature_id, signature_kind), variables)?
    } else {
        check_data_equation_infer(state, context, item_id, variables)?
    };

    let constructors = check_data_constructors(state, context, item_id)?;
    let declared_roles = Arc::clone(declared_roles);

    scc.data.push((item_id, PendingDataType { parameters, constructors, declared_roles }));

    Ok(())
}

fn check_data_equation_check<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature: (lowering::TypeId, TypeId),
    bindings: &[TypeVariableBinding],
) -> QueryResult<Vec<ForallBinder>>
where
    Q: ExternalQueries,
{
    let signature = signature::expect_type_signature(state, context, signature, bindings)?;
    let arguments = signature.arguments().collect_vec();
    check_type_variable_bindings(state, context, bindings, &arguments)
}

fn check_type_variable_bindings<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[TypeVariableBinding],
    signature: &[TypeId],
) -> QueryResult<Vec<ForallBinder>>
where
    Q: ExternalQueries,
{
    let mut binders = vec![];

    for (index, equation_binding) in bindings.iter().enumerate() {
        let signature_kind = signature.get(index).copied();

        let kind = resolve_type_variable_binding(state, context, signature_kind, equation_binding)?;

        let name = state.names.fresh();
        state.checked.node_types.forall_bindings.insert(equation_binding.id, kind);
        state.bindings.bind_forall(equation_binding.id, name, state.depth, kind);

        let text = if let Some(name) = &equation_binding.name {
            SmolStr::clone(name)
        } else {
            name.as_text()
        };

        let text = context.queries.intern_smol_str(text);
        state.checked.names.insert(name, text);
        let visible = equation_binding.visible;

        binders.push(ForallBinder { visible, name, kind, scope: None });
    }

    Ok(binders)
}

fn resolve_type_variable_binding<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    signature: Option<TypeId>,
    binding: &TypeVariableBinding,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    match (signature, binding.kind) {
        (Some(signature_kind), Some(binding_kind)) => {
            let (binding_kind, _) = types::infer_kind(state, context, binding_kind)?;
            let valid = unification::subtype(state, context, signature_kind, binding_kind)?;
            if valid { Ok(binding_kind) } else { Ok(context.unknown("invalid variable kind")) }
        }
        (Some(signature_kind), None) => {
            // Pure checking
            Ok(signature_kind)
        }
        (None, Some(binding_kind)) => {
            let (binding_kind, _) =
                types::check_kind(state, context, binding_kind, context.prim.t)?;
            Ok(binding_kind)
        }
        (None, None) => {
            // Pure inference
            Ok(state.fresh_unification(context.queries, context.prim.t))
        }
    }
}

fn check_data_equation_infer<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
    bindings: &[TypeVariableBinding],
) -> QueryResult<Vec<ForallBinder>>
where
    Q: ExternalQueries,
{
    let bindings = check_type_variable_bindings(state, context, bindings, &[])?;
    let kinds = bindings.iter().map(|binder| binder.kind);
    let inferred = context.intern_function_iter(kinds, context.prim.t);

    if let Some(expected) = state.checked.lookup_type_item_kind(item_id) {
        unification::subtype(state, context, inferred, expected)?;
    } else {
        state.checked.type_item_kinds.insert(item_id, inferred);
    }

    Ok(bindings)
}

fn check_data_constructors<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
) -> QueryResult<Vec<(TermItemId, Vec<TypeId>)>>
where
    Q: ExternalQueries,
{
    let mut constructors = vec![];

    for constructor_id in context.indexed.data_constructors(item_id) {
        let Some(TermItemKind::Constructor { arguments }) =
            context.lowered.tree.get_term_item_kind(constructor_id)
        else {
            continue;
        };

        let mut checked_arguments = vec![];
        for &argument in arguments.iter() {
            state.with_error_crumb(ErrorCrumb::ConstructorArgument(argument), |state| {
                let (checked_argument, _) =
                    types::check_kind(state, context, argument, context.prim.t)?;
                checked_arguments.push(checked_argument);
                Ok(())
            })?;
        }
        constructors.push((constructor_id, checked_arguments));
    }

    Ok(constructors)
}

fn finalise_data_declarations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (item_id, PendingDataType { parameters, constructors, .. }) in mem::take(&mut scc.data) {
        // constructor_kind should have already been generalised by the
        // finalise_binding_group function. The kind signature is used
        // as the source of truth for constructing kind applications.
        let Some(constructor_kind) = state.checked.type_item_kinds.get(&item_id).copied() else {
            continue;
        };

        let signature = signature::decompose_signature(
            state,
            context,
            constructor_kind,
            signature::DecomposeSignatureMode::Full,
        )?;
        let parameter_kinds = signature.arguments().collect_vec();
        let abstractions = signature.abstractions;
        let kind_binders = abstractions.into_iter().filter_map(|abstraction| {
            let signature::DecomposedAbstraction::Type { binder } = abstraction else {
                return None;
            };
            Some(context.lookup_forall_binder(binder))
        });
        let kind_binders = kind_binders.collect::<Vec<_>>();

        // parameter_kinds is the post-generalisation kind for each parameter;
        // we want to replace pre-generalisation kinds carried by parameters
        // before constructing the signature for the constructor.
        let get_parameter_kind = |index: usize| {
            if let Some(kind) = parameter_kinds.get(index) {
                *kind
            } else {
                context.unknown("invalid kind")
            }
        };

        // For the following code, let's trace through the declaration:
        //
        //   newtype Tagged :: forall k. k -> Type -> Type
        //
        let mut type_reference =
            context.queries.intern_type(Type::Constructor(context.id, item_id));

        // Tagged @k
        for binder in &kind_binders {
            let rigid = context.intern_rigid(binder.name, state.depth, binder.kind);
            type_reference = context.intern_kind_application(type_reference, rigid);
        }

        let type_parameters = parameters.iter().copied().enumerate().map(|(index, parameter)| {
            let kind = get_parameter_kind(index);
            let binder = ForallBinder { kind, ..parameter };
            context.intern_forall_binder(binder)
        });

        let type_parameters = type_parameters.collect::<Arc<[_]>>();
        let mut semantic_constructors = vec![];

        for (constructor_id, checked_arguments) in constructors {
            let mut result = type_reference;

            // Tagged @k t a
            for (index, parameter) in parameters.iter().enumerate() {
                let kind = get_parameter_kind(index);
                let rigid = context.intern_rigid(parameter.name, state.depth, kind);
                result = context.intern_application(result, rigid);
            }

            let mut arguments = Vec::with_capacity(checked_arguments.len());
            for argument in checked_arguments {
                let argument = zonk::zonk(state, context, argument)?;
                let argument = ApplyKinds::on(state, context, item_id, type_reference, argument)?;
                arguments.push(argument);
            }

            // a -> Tagged @k t a
            for &argument in arguments.iter().rev() {
                result = context.intern_function(argument, result);
            }

            // forall (a :: Type). a -> Tagged @k t a
            for type_parameter in type_parameters.iter().rev() {
                result = context.intern_forall(*type_parameter, result);
            }

            // forall (k :: Type) (t :: k) (a :: Type). a -> Tagged @k t a
            for binder in kind_binders.iter().rev() {
                let binder = ForallBinder { visible: false, ..*binder };
                let binder_id = context.intern_forall_binder(binder);
                result = context.intern_forall(binder_id, result);
            }

            state.checked.term_item_types.insert(constructor_id, result);

            let constructor = tree::DataConstructor { arguments: Arc::from(arguments) };
            let declaration = tree::TermDeclaration {
                type_id: result,
                kind: tree::TermDeclarationKind::Constructor(constructor),
            };

            let declaration = state.checked.tree.insert_term(constructor_id, declaration);
            semantic_constructors.push(declaration);
        }

        let checked_declaration =
            CheckedDataDeclaration { type_parameters: Arc::clone(&type_parameters) };
        state.checked.data_declarations.insert(item_id, checked_declaration);

        let data = tree::DataDeclaration {
            parameters: type_parameters,
            constructors: Arc::from(semantic_constructors),
        };

        let declaration = match &context.indexed.items[item_id].kind {
            IndexedTypeItemKind::Data { .. } => tree::TypeDeclarationKind::Data(data),
            IndexedTypeItemKind::Newtype { .. } => tree::TypeDeclarationKind::Newtype(data),
            _ => unreachable!("invariant violated: pending data type is not data or newtype"),
        };

        let roles = state.checked.lookup_roles(item_id).unwrap_or_default();
        let declaration = tree::TypeDeclaration { kind: constructor_kind, roles, declaration };
        state.checked.tree.insert_type_declaration(item_id, declaration);
    }

    Ok(())
}

fn finalise_roles<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (item_id, pending) in &scc.data {
        let PendingDataType { parameters, constructors, declared_roles } = pending;
        let inferred_roles =
            super::roles::infer_data_roles(state, context, parameters, constructors)?;
        let resolved_roles = super::roles::check_declared_roles(
            state,
            *item_id,
            &inferred_roles,
            declared_roles,
            false,
        );
        state.checked.roles.insert(*item_id, resolved_roles);
    }

    for (item_id, declared_roles) in mem::take(&mut scc.foreign) {
        let Some(kind) = state.checked.lookup_type_item_kind(item_id) else {
            continue;
        };

        let parameter_count = super::roles::count_kind_arguments(state, context, kind)?;
        let inferred_roles = vec![Role::Nominal; parameter_count];
        let resolved_roles = super::roles::check_declared_roles(
            state,
            item_id,
            &inferred_roles,
            &declared_roles,
            true,
        );

        state.checked.roles.insert(item_id, resolved_roles);
    }

    Ok(())
}

fn check_synonym_equation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
    item_id: TypeItemId,
    signature: Option<lowering::TypeId>,
    bindings: &[TypeVariableBinding],
    synonym: Option<lowering::TypeId>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let (parameters, result) = if let Some(signature_id) = signature
        && let Some(signature_kind) = state.checked.lookup_type_item_kind(item_id)
    {
        check_synonym_equation_check(state, context, bindings, (signature_id, signature_kind))?
    } else {
        check_synonym_equation_infer(state, context, item_id, bindings)?
    };

    let synonym = if let Some(synonym) = synonym {
        let (synonym, _) = types::check_kind(state, context, synonym, result)?;
        synonym
    } else {
        context.unknown("invalid synonym type")
    };

    scc.synonym.push((item_id, PendingSynonymType { parameters, synonym }));

    Ok(())
}

fn check_synonym_equation_check<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[TypeVariableBinding],
    (signature_id, signature_kind): (lowering::TypeId, TypeId),
) -> QueryResult<(Vec<ForallBinder>, TypeId)>
where
    Q: ExternalQueries,
{
    let signature =
        signature::expect_type_signature(state, context, (signature_id, signature_kind), bindings)?;
    let arguments = signature.arguments().collect_vec();
    let parameters = check_type_variable_bindings(state, context, bindings, &arguments)?;
    Ok((parameters, signature.result))
}

fn check_synonym_equation_infer<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
    bindings: &[TypeVariableBinding],
) -> QueryResult<(Vec<ForallBinder>, TypeId)>
where
    Q: ExternalQueries,
{
    let bindings = check_type_variable_bindings(state, context, bindings, &[])?;
    let kinds = bindings.iter().map(|binder| binder.kind);
    let result = state.fresh_unification(context.queries, context.prim.t);
    let inferred = context.intern_function_iter(kinds, result);

    if let Some(expected) = state.checked.lookup_type_item_kind(item_id) {
        unification::subtype(state, context, inferred, expected)?;
    } else {
        state.checked.type_item_kinds.insert(item_id, inferred);
    }

    Ok((bindings, result))
}

fn finalise_synonym_replacements<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (item_id, PendingSynonymType { parameters, synonym }) in mem::take(&mut scc.synonym) {
        let Some(kind) = state.checked.lookup_type_item_kind(item_id) else {
            continue;
        };
        let synonym = zonk::zonk(state, context, synonym)?;
        let synonym = CheckedSynonym { kind, parameters, expansion: synonym };
        state.checked.synonyms.insert(item_id, synonym);
    }
    Ok(())
}

fn check_class_equation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
    item_id: TypeItemId,
    signature: Option<lowering::TypeId>,
    declaration: &ClassDeclaration,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let ClassDeclaration { constraints, variables, functional_dependencies } = declaration;

    let parameters = if let Some(signature_id) = signature
        && let Some(signature_kind) = state.checked.lookup_type_item_kind(item_id)
    {
        check_class_equation_check(state, context, variables, (signature_id, signature_kind))?
    } else {
        check_class_equation_infer(state, context, item_id, variables)?
    };

    let mut superclasses = vec![];
    for &source_id in constraints.iter() {
        let (constraint, _) =
            types::check_kind(state, context, source_id, context.prim.constraint)?;
        superclasses.push(CheckedSuperclass { source_id, constraint });
    }

    let functional_dependencies =
        functional_dependencies.iter().map(fd::Fd::from_lowering).collect();

    let members = check_class_members(state, context, item_id)?;

    scc.class.push((
        item_id,
        PendingClassType { parameters, superclasses, functional_dependencies, members },
    ));

    Ok(())
}

fn check_class_equation_check<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    bindings: &[TypeVariableBinding],
    signature: (lowering::TypeId, TypeId),
) -> QueryResult<Vec<ForallBinder>>
where
    Q: ExternalQueries,
{
    let signature = signature::expect_type_signature(state, context, signature, bindings)?;
    let arguments = signature.arguments().collect_vec();
    check_type_variable_bindings(state, context, bindings, &arguments)
}

fn check_class_equation_infer<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
    bindings: &[TypeVariableBinding],
) -> QueryResult<Vec<ForallBinder>>
where
    Q: ExternalQueries,
{
    let bindings = check_type_variable_bindings(state, context, bindings, &[])?;
    let kinds = bindings.iter().map(|binder| binder.kind);
    let inferred = context.intern_function_iter(kinds, context.prim.constraint);

    if let Some(expected) = state.checked.lookup_type_item_kind(item_id) {
        unification::subtype(state, context, inferred, expected)?;
    } else {
        state.checked.type_item_kinds.insert(item_id, inferred);
    }

    Ok(bindings)
}

fn check_class_members<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
) -> QueryResult<Vec<(TermItemId, TypeId)>>
where
    Q: ExternalQueries,
{
    let mut members = vec![];

    for member_id in context.indexed.class_members(item_id) {
        let Some(TermItemKind::ClassMember { signature }) =
            context.lowered.tree.get_term_item_kind(member_id)
        else {
            continue;
        };

        let Some(signature_id) = signature else { continue };

        let (member_type, _) = types::check_kind(state, context, *signature_id, context.prim.t)?;
        members.push((member_id, member_type));
    }

    Ok(members)
}

fn finalise_classes<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    scc: &mut TypeSccState,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for (item_id, pending) in mem::take(&mut scc.class) {
        let PendingClassType { parameters, superclasses, functional_dependencies, members } =
            pending;

        let Some(class_kind) = state.checked.type_item_kinds.get(&item_id).copied() else {
            continue;
        };

        let signature = signature::decompose_signature(
            state,
            context,
            class_kind,
            signature::DecomposeSignatureMode::Full,
        )?;
        let class_parameters = signature.arguments().collect_vec();
        let abstractions = signature.abstractions;
        let class_binders = abstractions.into_iter().filter_map(|abstraction| {
            let signature::DecomposedAbstraction::Type { binder } = abstraction else {
                return None;
            };
            Some(context.lookup_forall_binder(binder))
        });
        let class_binders = class_binders.collect::<Vec<_>>();

        let get_parameter_kind = |index: usize| {
            if let Some(kind) = class_parameters.get(index) {
                *kind
            } else {
                context.unknown("invalid kind")
            }
        };

        let kind_binders = class_binders
            .iter()
            .copied()
            .map(|binder| context.intern_forall_binder(binder))
            .collect::<Arc<[_]>>();

        let type_parameters = parameters
            .iter()
            .copied()
            .enumerate()
            .map(|(index, parameter)| {
                let kind = get_parameter_kind(index);
                let binder = ForallBinder { kind, ..parameter };
                context.intern_forall_binder(binder)
            })
            .collect::<Arc<[_]>>();

        let mut class_head = context.queries.intern_type(Type::Constructor(context.id, item_id));

        for binder in &class_binders {
            let rigid = context.intern_rigid(binder.name, state.depth, binder.kind);
            class_head = context.intern_kind_application(class_head, rigid);
        }

        for (index, parameter) in parameters.iter().enumerate() {
            let kind = get_parameter_kind(index);
            let rigid = context.intern_rigid(parameter.name, state.depth, kind);
            class_head = context.intern_application(class_head, rigid);
        }

        let mut canonical = class_head;
        for type_parameter in type_parameters.iter().rev() {
            canonical = context.intern_forall(*type_parameter, canonical);
        }
        for kind_binder in kind_binders.iter().rev() {
            canonical = context.intern_forall(*kind_binder, canonical);
        }

        let superclasses = superclasses
            .into_iter()
            .map(|superclass| {
                let constraint = zonk::zonk(state, context, superclass.constraint)?;
                Ok(CheckedSuperclass { source_id: superclass.source_id, constraint })
            })
            .collect::<QueryResult<Vec<_>>>()?;

        let semantic_superclasses = superclasses.iter().map(|superclass| tree::ClassSuperclass {
            id: SuperclassId {
                file_id: context.id,
                type_id: item_id,
                source_id: superclass.source_id,
            },
            constraint: superclass.constraint,
        });
        let semantic_superclasses = semantic_superclasses.collect::<Arc<[_]>>();

        let mut checked_members = Vec::with_capacity(members.len());
        let mut semantic_members = Vec::with_capacity(members.len());
        for (member_id, member_type) in members {
            let field_type = zonk::zonk(state, context, member_type)?;
            let mut selector_type = context.intern_constrained(class_head, field_type);

            for type_parameter in type_parameters.iter().rev() {
                selector_type = context.intern_forall(*type_parameter, selector_type);
            }

            for kind_binder in kind_binders.iter().rev() {
                selector_type = context.intern_forall(*kind_binder, selector_type);
            }

            state.checked.term_item_types.insert(member_id, selector_type);
            checked_members.push(CheckedClassMember { item_id: member_id, field_type });
            semantic_members.push(tree::ClassMember { source: member_id, field_type });
        }

        state.checked.classes.insert(
            item_id,
            CheckedClass {
                kind_binders: Arc::clone(&kind_binders),
                type_parameters: Arc::clone(&type_parameters),
                canonical,
                superclasses,
                functional_dependencies,
                members: checked_members,
            },
        );

        let class = tree::ClassDeclaration {
            kind_binders,
            type_parameters,
            superclasses: semantic_superclasses,
            members: Arc::from(semantic_members),
        };
        let declaration = tree::TypeDeclaration {
            kind: class_kind,
            roles: Arc::default(),
            declaration: tree::TypeDeclarationKind::Class(class),
        };
        state.checked.tree.insert_type_declaration(item_id, declaration);
    }

    Ok(())
}

fn check_type_operator<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item_id: TypeItemId,
    resolution: Option<(FileId, TypeItemId)>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some((file_id, type_id)) = resolution else { return Ok(()) };
    let operator_kind = toolkit::lookup_file_type_operator(state, context, file_id, type_id)?;

    if let Some(item_kind) = state.checked.lookup_type_item_kind(item_id) {
        unification::subtype(state, context, operator_kind, item_kind)?;
    } else {
        state.checked.type_item_kinds.insert(item_id, operator_kind);
    }

    Ok(())
}
