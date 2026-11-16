use std::sync::Arc;

use building_types::QueryResult;
use indexing::{TermItemId, TypeItemId};
use itertools::Itertools;
use lowering::{TermItemKind, TypeItemKind, TypeVariableBinding};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};

use crate::context::CheckContext;
use crate::core::{fold, kind, normalise, unify};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::{roles, types};
use crate::state::{CheckState, TypeVariableKey};
use crate::types::{ForallBinder, ForallBinderId, Type, TypeId, UnificationId};
use crate::{
    CheckedClass, CheckedConstructor, CheckedData, CheckedSynonym, ExternalQueries,
    FunctionalDependency,
};

pub fn check_type_items<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for scc in context.grouped.type_scc.iter() {
        check_type_group(state, context, scc.as_slice())?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum Declaration {
    Data {
        newtype: bool,
        parameters: Vec<ForallBinderId>,
        kind_binders: Vec<ForallBinderId>,
        constructors: Vec<(TermItemId, Vec<TypeId>)>,
    },
    Synonym {
        parameters: Vec<ForallBinderId>,
        kind_binders: Vec<ForallBinderId>,
        body: TypeId,
    },
    Class {
        parameters: Vec<ForallBinderId>,
        kind_binders: Vec<ForallBinderId>,
        superclasses: Vec<TypeId>,
        functional_dependencies: Vec<FunctionalDependency>,
        members: Vec<(TermItemId, TypeId)>,
    },
    Foreign,
}

struct Parameters {
    kind_binders: Vec<ForallBinderId>,
    parameters: Vec<ForallBinderId>,
    result_kind: Option<TypeId>,
}

fn check_type_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TypeItemId],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let items = items.iter().copied().filter(|&item| {
        let kind = context.lowered.tree.get_type_item_kind(item);
        !matches!(kind, Some(TypeItemKind::Operator { .. }) | None)
    });
    let items = items.collect_vec();
    if items.is_empty() {
        return Ok(());
    }

    let declarations = state.with_level(|state| infer_type_group(state, context, &items))?;

    generalise_type_group(state, context, &items, declarations)?;

    roles::infer_group_roles(state, context, &items)?;

    Ok(())
}

fn item_signature<Q>(context: &CheckContext<Q>, item: TypeItemId) -> Option<lowering::TypeId>
where
    Q: ExternalQueries,
{
    match context.lowered.tree.get_type_item_kind(item)? {
        TypeItemKind::Data { signature, .. }
        | TypeItemKind::Newtype { signature, .. }
        | TypeItemKind::Synonym { signature, .. }
        | TypeItemKind::Class { signature, .. }
        | TypeItemKind::Foreign { signature, .. } => *signature,
        TypeItemKind::Operator { .. } => None,
    }
}

fn infer_type_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TypeItemId],
) -> QueryResult<Vec<Declaration>>
where
    Q: ExternalQueries,
{
    for &item in items {
        let kind = kind::fresh_type(state, context);
        state.pending_types.insert(item, kind);
    }

    let mut signatures = FxHashMap::default();
    for &item in items {
        let Some(signature) = item_signature(context, item) else {
            continue;
        };
        state.with_crumb(ErrorCrumb::TypeDeclaration(item), |state| {
            let kind = types::check_type(state, context, signature, context.types.type_)?;
            let pending = pending_kind(state, item);
            unify::unify_kinds(state, context, pending, kind)?;
            signatures.insert(item, kind);
            QueryResult::Ok(())
        })?;
    }

    let synonyms = items.iter().copied().filter(|&item| {
        matches!(context.lowered.tree.get_type_item_kind(item), Some(TypeItemKind::Synonym { .. }))
    });
    let synonyms = synonym_order(context, synonyms.collect_vec());

    let mut declarations = FxHashMap::default();
    for item in synonyms {
        let declaration = state.with_crumb(ErrorCrumb::TypeDeclaration(item), |state| {
            infer_synonym(state, context, item, signatures.get(&item).copied())
        })?;
        declarations.insert(item, declaration);
    }

    for &item in items {
        if declarations.contains_key(&item) {
            continue;
        }
        let declaration = state.with_crumb(ErrorCrumb::TypeDeclaration(item), |state| {
            infer_declaration(state, context, item, signatures.get(&item).copied())
        })?;
        declarations.insert(item, declaration);
    }

    let declarations = items.iter().map(|item| {
        declarations.remove(item).expect("invariant violated: missing type declaration")
    });
    Ok(declarations.collect_vec())
}

fn synonym_order<Q>(context: &CheckContext<Q>, synonyms: Vec<TypeItemId>) -> Vec<TypeItemId>
where
    Q: ExternalQueries,
{
    let mut ordered = Vec::with_capacity(synonyms.len());
    let mut visited = FxHashSet::default();
    let members = synonyms.iter().copied().collect::<FxHashSet<_>>();

    fn visit<Q>(
        context: &CheckContext<Q>,
        item: TypeItemId,
        members: &FxHashSet<TypeItemId>,
        visited: &mut FxHashSet<TypeItemId>,
        ordered: &mut Vec<TypeItemId>,
    ) where
        Q: ExternalQueries,
    {
        if !visited.insert(item) {
            return;
        }
        let dependencies = context.lowered.synonym_edges.iter().filter(|(from, _)| *from == item);
        for &(_, to) in dependencies {
            if members.contains(&to) {
                visit(context, to, members, visited, ordered);
            }
        }
        ordered.push(item);
    }

    for &item in &synonyms {
        visit(context, item, &members, &mut visited, &mut ordered);
    }
    ordered
}

fn check_parameters<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    variables: &[TypeVariableBinding],
    signature: Option<TypeId>,
) -> QueryResult<Parameters>
where
    Q: ExternalQueries,
{
    let mut kind_binders = Vec::new();
    let mut signature_kind = signature;

    if let Some(kind) = signature_kind {
        let mut current = kind;
        loop {
            let head = normalise::normalise(state, context, current);
            let Type::Forall(binder_id, inner) = context.lookup_type(head) else {
                break;
            };
            kind_binders.push(binder_id);
            current = inner;
        }
        signature_kind = Some(current);
    }

    let mut seen = FxHashSet::default();
    let mut parameters = Vec::with_capacity(variables.len());
    for variable in variables {
        if let Some(name) = &variable.name
            && !seen.insert(SmolStr::clone(name))
        {
            let name = SmolStr::clone(name);
            state.insert_error(ErrorKind::DuplicateTypeArgument { name });
        }

        let annotation = match variable.kind {
            Some(kind) => Some(types::check_type(state, context, kind, context.types.type_)?),
            None => None,
        };

        let kind = match signature_kind {
            Some(current) => match normalise::function_parts(state, context, current) {
                Some((argument_kind, result_kind)) => {
                    signature_kind = Some(result_kind);
                    if let Some(annotation) = annotation {
                        unify::unify_kinds(state, context, argument_kind, annotation)?;
                    }
                    argument_kind
                }
                None => {
                    let argument_kind =
                        annotation.unwrap_or_else(|| kind::fresh_type(state, context));
                    let result_kind = kind::fresh_type(state, context);
                    let expected = context.intern_function(argument_kind, result_kind);
                    unify::unify_kinds(state, context, current, expected)?;
                    signature_kind = Some(result_kind);
                    argument_kind
                }
            },
            None => match annotation {
                Some(annotation) => annotation,
                None => kind::fresh_type(state, context),
            },
        };

        let text = variable.name.as_ref().map(SmolStr::clone);
        let text = text.unwrap_or_else(|| SmolStr::new_static("_"));
        let text = context.intern_text(text);
        let name = state.fresh_name();
        let binder = ForallBinder { visible: variable.visible, name, text, kind };
        let binder_id = context.intern_forall_binder(binder);
        let variable_type = context.intern_type(Type::Variable(binder_id));
        state.type_variables.insert(TypeVariableKey::Forall(variable.id), variable_type);
        parameters.push(binder_id);
    }

    Ok(Parameters { kind_binders, parameters, result_kind: signature_kind })
}

fn parameter_kinds<Q>(context: &CheckContext<Q>, parameters: &[ForallBinderId]) -> Vec<TypeId>
where
    Q: ExternalQueries,
{
    parameters.iter().map(|&binder_id| context.lookup_forall_binder(binder_id).kind).collect_vec()
}

fn unify_declared_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TypeItemId,
    parameters: &Parameters,
    result_kind: TypeId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    match parameters.result_kind {
        Some(signature_result) => {
            unify::unify_kinds(state, context, signature_result, result_kind)?;
        }
        None => {
            let kinds = parameter_kinds(context, &parameters.parameters);
            let kind = context.intern_function_list(&kinds, result_kind);
            let pending = pending_kind(state, item);
            unify::unify_kinds(state, context, pending, kind)?;
        }
    }
    Ok(())
}

fn infer_synonym<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TypeItemId,
    signature: Option<TypeId>,
) -> QueryResult<Declaration>
where
    Q: ExternalQueries,
{
    let Some(TypeItemKind::Synonym { declaration: Some(declaration), .. }) =
        context.lowered.tree.get_type_item_kind(item)
    else {
        return Ok(Declaration::Foreign);
    };

    let parameters = check_parameters(state, context, &declaration.variables, signature)?;

    let pending = CheckedSynonym {
        kind_binders: Arc::from([]),
        parameters: Arc::from(parameters.parameters.as_slice()),
        body: context.types.unknown,
    };
    state.pending_synonyms.insert(item, pending);

    let (body, body_kind) = match declaration.type_ {
        Some(type_) => match parameters.result_kind {
            Some(result_kind) => {
                let body = types::check_type(state, context, type_, result_kind)?;
                (body, result_kind)
            }
            None => types::infer_type(state, context, type_)?,
        },
        None => (context.types.unknown, context.types.unknown),
    };

    unify_declared_kind(state, context, item, &parameters, body_kind)?;

    let synonym = CheckedSynonym {
        kind_binders: Arc::from(parameters.kind_binders.as_slice()),
        parameters: Arc::from(parameters.parameters.as_slice()),
        body,
    };
    state.pending_synonyms.insert(item, synonym);

    Ok(Declaration::Synonym {
        parameters: parameters.parameters,
        kind_binders: parameters.kind_binders,
        body,
    })
}

fn infer_declaration<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TypeItemId,
    signature: Option<TypeId>,
) -> QueryResult<Declaration>
where
    Q: ExternalQueries,
{
    let Some(kind) = context.lowered.tree.get_type_item_kind(item) else {
        return Ok(Declaration::Foreign);
    };

    match kind {
        TypeItemKind::Data { declaration, .. } => {
            let variables = declaration.as_ref().map(|declaration| &declaration.variables[..]);
            infer_data(state, context, item, false, variables, signature)
        }
        TypeItemKind::Newtype { declaration, .. } => {
            let variables = declaration.as_ref().map(|declaration| &declaration.variables[..]);
            infer_data(state, context, item, true, variables, signature)
        }
        TypeItemKind::Class { declaration, .. } => {
            infer_class(state, context, item, declaration.as_ref(), signature)
        }
        TypeItemKind::Foreign { .. }
        | TypeItemKind::Synonym { .. }
        | TypeItemKind::Operator { .. } => Ok(Declaration::Foreign),
    }
}

fn infer_data<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TypeItemId,
    newtype: bool,
    variables: Option<&[TypeVariableBinding]>,
    signature: Option<TypeId>,
) -> QueryResult<Declaration>
where
    Q: ExternalQueries,
{
    let variables = variables.unwrap_or_default();
    let parameters = check_parameters(state, context, variables, signature)?;

    let mut constructors = Vec::new();
    for constructor in context.indexed.data_constructors(item) {
        let Some(TermItemKind::Constructor { arguments }) =
            context.lowered.tree.get_term_item_kind(constructor)
        else {
            continue;
        };
        let mut fields = Vec::with_capacity(arguments.len());
        state.with_crumb(ErrorCrumb::TermDeclaration(constructor), |state| {
            for &argument in arguments.iter() {
                fields.push(types::check_type(state, context, argument, context.types.type_)?);
            }
            QueryResult::Ok(())
        })?;
        constructors.push((constructor, fields));
    }

    let declared_only = constructors.is_empty() && signature.is_some();
    if !declared_only {
        unify_declared_kind(state, context, item, &parameters, context.types.type_)?;
    }

    Ok(Declaration::Data {
        newtype,
        parameters: parameters.parameters,
        kind_binders: parameters.kind_binders,
        constructors,
    })
}

fn infer_class<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TypeItemId,
    declaration: Option<&lowering::ClassDeclaration>,
    signature: Option<TypeId>,
) -> QueryResult<Declaration>
where
    Q: ExternalQueries,
{
    let variables = declaration.map(|declaration| &declaration.variables[..]).unwrap_or_default();
    let parameters = check_parameters(state, context, variables, signature)?;

    let mut superclasses = Vec::new();
    if let Some(declaration) = declaration {
        for &constraint in declaration.constraints.iter() {
            let constraint =
                types::check_type(state, context, constraint, context.types.constraint)?;
            superclasses.push(constraint);
        }
    }

    let mut members = Vec::new();
    for member in context.indexed.class_members(item) {
        let Some(TermItemKind::ClassMember { signature: Some(signature) }) =
            context.lowered.tree.get_term_item_kind(member)
        else {
            continue;
        };
        let member_type = state.with_crumb(ErrorCrumb::TermDeclaration(member), |state| {
            types::check_type(state, context, *signature, context.types.type_)
        })?;
        members.push((member, member_type));
    }

    let functional_dependencies = declaration.map_or_else(Vec::new, |declaration| {
        let dependencies = declaration.functional_dependencies.iter().map(|dependency| {
            let determiners = dependency.determiners.iter().map(|&index| usize::from(index));
            let determined = dependency.determined.iter().map(|&index| usize::from(index));
            FunctionalDependency {
                determiners: determiners.collect::<Arc<[_]>>(),
                determined: determined.collect::<Arc<[_]>>(),
            }
        });
        dependencies.collect_vec()
    });

    if declaration.is_some() {
        unify_declared_kind(state, context, item, &parameters, context.types.constraint)?;
    }

    Ok(Declaration::Class {
        parameters: parameters.parameters,
        kind_binders: parameters.kind_binders,
        superclasses,
        functional_dependencies,
        members,
    })
}

struct GroupGeneralisation {
    unknowns: FxHashMap<TypeItemId, Vec<UnificationId>>,
}

fn generalise_type_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TypeItemId],
    declarations: Vec<Declaration>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut generalisation = GroupGeneralisation { unknowns: FxHashMap::default() };
    let mut mappings = FxHashMap::default();
    let mut kinds = FxHashMap::default();

    for &item in items {
        let pending = pending_kind(state, item);
        let pending = fold::zonk(state, context, pending);
        let unknowns = generalisable_unknowns(state, context, pending);

        let mut mapping = FxHashMap::default();
        let mut binders = Vec::with_capacity(unknowns.len());
        for (index, &unification_id) in unknowns.iter().enumerate() {
            let kind = state.unification(unification_id).kind;
            let kind = finalise_kind(state, context, &mapping, kind);
            let text =
                if index == 0 { SmolStr::new_static("k") } else { format_smolstr!("k{index}") };
            let text = context.intern_text(text);
            let name = state.fresh_name();
            let binder = ForallBinder { visible: false, name, text, kind };
            let binder_id = context.intern_forall_binder(binder);
            mapping.insert(unification_id, context.intern_type(Type::Variable(binder_id)));
            binders.push(binder_id);
        }

        let kind = finalise_kind(state, context, &mapping, pending);
        let kind = context.intern_forall_list(&binders, kind);
        kinds.insert(item, (kind, binders));
        mappings.insert(item, mapping);
        generalisation.unknowns.insert(item, unknowns);
    }

    for (&item, declaration) in items.iter().zip(declarations) {
        let mapping = mappings.get(&item).expect("invariant violated: missing group mapping");
        let (kind, binders) = kinds.remove(&item).expect("invariant violated: missing group kind");
        let finalise = |state: &CheckState, t: TypeId| {
            finalise_type(state, context, &generalisation, mapping, t)
        };
        let finalise_binder = |state: &CheckState, binder_id: ForallBinderId| {
            let binder = context.lookup_forall_binder(binder_id);
            let kind = finalise(state, binder.kind);
            context.intern_forall_binder(ForallBinder { kind, ..binder })
        };

        state.checked.type_kinds.insert(item, kind);

        match declaration {
            Declaration::Data { newtype, parameters, kind_binders, constructors } => {
                let kind_binders =
                    kind_binders.iter().map(|&binder_id| finalise_binder(state, binder_id));
                let kind_binders = binders.iter().copied().chain(kind_binders).collect_vec();
                let parameters =
                    parameters.iter().map(|&binder_id| finalise_binder(state, binder_id));
                let parameters = parameters.collect_vec();
                let parameter_substitution = parameter_substitution(context, &parameters);

                let head = context.intern_constructor((context.id, item));
                let head = kind_binders.iter().fold(head, |head, &binder_id| {
                    let variable = context.intern_type(Type::Variable(binder_id));
                    context.intern_kind_application(head, variable)
                });
                let arguments = parameters
                    .iter()
                    .map(|&binder_id| context.intern_type(Type::Variable(binder_id)));
                let arguments = arguments.collect_vec();
                let head = context.intern_application_list(head, &arguments);

                let mut checked_constructors = Vec::with_capacity(constructors.len());
                for (constructor, fields) in constructors {
                    let fields = fields.iter().map(|&field| {
                        let field = finalise(state, field);
                        fold::substitute_variables(context, &parameter_substitution, field)
                    });
                    let fields = fields.collect::<Arc<[_]>>();
                    let constructor_type = context.intern_function_list(&fields, head);
                    let constructor_type =
                        context.intern_forall_list(&parameters, constructor_type);
                    let constructor_type =
                        context.intern_forall_list(&kind_binders, constructor_type);
                    let crumb = ErrorCrumb::TermDeclaration(constructor);
                    check_type_quantification(state, context, crumb, constructor_type);
                    state.checked.term_types.insert(constructor, constructor_type);
                    checked_constructors.push(CheckedConstructor { item: constructor, fields });
                }

                let data = CheckedData {
                    newtype,
                    kind_binders: Arc::from(kind_binders),
                    parameters: Arc::from(parameters),
                    constructors: Arc::from(checked_constructors),
                };
                state.checked.data.insert(item, data);
            }
            Declaration::Synonym { parameters, kind_binders, body } => {
                let kind_binders =
                    kind_binders.iter().map(|&binder_id| finalise_binder(state, binder_id));
                let kind_binders = binders.iter().copied().chain(kind_binders).collect_vec();
                let parameters =
                    parameters.iter().map(|&binder_id| finalise_binder(state, binder_id));
                let parameters = parameters.collect_vec();
                let parameter_substitution = parameter_substitution(context, &parameters);
                let body = finalise(state, body);
                let body = fold::substitute_variables(context, &parameter_substitution, body);
                let crumb = ErrorCrumb::TypeDeclaration(item);
                check_type_quantification(state, context, crumb, body);
                let synonym = CheckedSynonym {
                    kind_binders: Arc::from(kind_binders),
                    parameters: Arc::from(parameters),
                    body,
                };
                state.pending_synonyms.remove(&item);
                state.checked.synonyms.insert(item, synonym);
            }
            Declaration::Class {
                parameters,
                kind_binders,
                superclasses,
                functional_dependencies,
                members,
            } => {
                let kind_binders =
                    kind_binders.iter().map(|&binder_id| finalise_binder(state, binder_id));
                let kind_binders = binders.iter().copied().chain(kind_binders).collect_vec();
                let parameters =
                    parameters.iter().map(|&binder_id| finalise_binder(state, binder_id));
                let parameters = parameters.collect_vec();
                let parameter_substitution = parameter_substitution(context, &parameters);
                let finalise_member = |state: &CheckState, t: TypeId| {
                    let t = finalise(state, t);
                    fold::substitute_variables(context, &parameter_substitution, t)
                };

                let superclasses =
                    superclasses.iter().map(|&superclass| finalise_member(state, superclass));
                let superclasses = superclasses.collect::<Arc<[_]>>();

                let head = context.intern_constructor((context.id, item));
                let head = kind_binders.iter().fold(head, |head, &binder_id| {
                    let variable = context.intern_type(Type::Variable(binder_id));
                    context.intern_kind_application(head, variable)
                });
                let arguments = parameters
                    .iter()
                    .map(|&binder_id| context.intern_type(Type::Variable(binder_id)));
                let arguments = arguments.collect_vec();
                let constraint = context.intern_application_list(head, &arguments);

                let mut checked_members = Vec::with_capacity(members.len());
                for (member, member_type) in members {
                    let member_type = finalise_member(state, member_type);
                    checked_members.push((member, member_type));
                    let full_type = context.intern_constrained(constraint, member_type);
                    let full_type = context.intern_forall_list(&parameters, full_type);
                    let full_type = context.intern_forall_list(&kind_binders, full_type);
                    state.checked.term_types.insert(member, full_type);
                }

                let class = CheckedClass {
                    kind_binders: Arc::from(kind_binders),
                    parameters: Arc::from(parameters),
                    superclasses,
                    functional_dependencies: Arc::from(functional_dependencies),
                    members: Arc::from(checked_members),
                };
                state.checked.classes.insert(item, class);
            }
            Declaration::Foreign => (),
        }
    }

    for &item in items {
        state.pending_types.remove(&item);
    }

    Ok(())
}

fn pending_kind(state: &CheckState, item: TypeItemId) -> TypeId {
    let pending = state.pending_types.get(&item).copied();
    pending.expect("invariant violated: type item without a pending kind")
}

fn check_type_quantification<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    crumb: ErrorCrumb,
    elaborated: TypeId,
) where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(elaborated).has_unification() {
        return;
    }
    state.with_crumb(crumb, |state| {
        state.insert_error(ErrorKind::QuantificationCheckFailureInType { elaborated });
    });
}

fn parameter_substitution<Q>(
    context: &CheckContext<Q>,
    parameters: &[ForallBinderId],
) -> FxHashMap<crate::types::Name, TypeId>
where
    Q: ExternalQueries,
{
    let substitution = parameters.iter().map(|&binder_id| {
        let binder = context.lookup_forall_binder(binder_id);
        (binder.name, context.intern_type(Type::Variable(binder_id)))
    });
    substitution.collect()
}

pub fn generalisable_unknowns<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> Vec<UnificationId>
where
    Q: ExternalQueries,
{
    let mut unknowns = Vec::new();
    collect_unknowns(state, context, id, &mut unknowns);
    let unknowns = unknowns
        .into_iter()
        .filter(|&unification_id| state.unification(unification_id).level > state.level);
    unknowns.unique().collect_vec()
}

pub fn collect_unknowns<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
    unknowns: &mut Vec<UnificationId>,
) where
    Q: ExternalQueries,
{
    let id = fold::zonk(state, context, id);
    if !context.lookup_type_flags(id).has_unification() {
        return;
    }
    if let Type::Unification(unification_id) = context.lookup_type(id) {
        let kind = state.unification(unification_id).kind;
        collect_unknowns(state, context, kind, unknowns);
        unknowns.push(unification_id);
        return;
    }
    fold::map_children(context, id, |child| {
        collect_unknowns(state, context, child, unknowns);
        child
    });
}

fn finalise_kind<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    mapping: &FxHashMap<UnificationId, TypeId>,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    let id = fold::zonk(state, context, id);
    replace_unknowns(context, mapping, id)
}

pub fn replace_unknowns<Q>(
    context: &CheckContext<Q>,
    mapping: &FxHashMap<UnificationId, TypeId>,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    if mapping.is_empty() || !context.lookup_type_flags(id).has_unification() {
        return id;
    }
    if let Type::Unification(unification_id) = context.lookup_type(id)
        && let Some(&replacement) = mapping.get(&unification_id)
    {
        return replacement;
    }
    fold::map_children(context, id, |child| replace_unknowns(context, mapping, child))
}

fn finalise_type<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    generalisation: &GroupGeneralisation,
    mapping: &FxHashMap<UnificationId, TypeId>,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    let id = fold::zonk(state, context, id);
    let id = insert_kind_applications(context, generalisation, mapping, id);
    replace_unknowns(context, mapping, id)
}

fn insert_kind_applications<Q>(
    context: &CheckContext<Q>,
    generalisation: &GroupGeneralisation,
    mapping: &FxHashMap<UnificationId, TypeId>,
    id: TypeId,
) -> TypeId
where
    Q: ExternalQueries,
{
    if let Type::Constructor(file_id, type_id) = context.lookup_type(id) {
        if file_id != context.id {
            return id;
        }
        let Some(unknowns) = generalisation.unknowns.get(&type_id) else {
            return id;
        };
        return unknowns.iter().fold(id, |function, &unification_id| {
            let argument = mapping.get(&unification_id).copied();
            let argument =
                argument.unwrap_or_else(|| context.intern_type(Type::Unification(unification_id)));
            context.intern_kind_application(function, argument)
        });
    }
    fold::map_children(context, id, |child| {
        insert_kind_applications(context, generalisation, mapping, child)
    })
}
