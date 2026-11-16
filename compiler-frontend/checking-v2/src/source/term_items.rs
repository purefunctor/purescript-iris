use std::mem;
use std::sync::Arc;

use building_types::QueryResult;
use indexing::TermItemId;
use itertools::Itertools;
use lowering::{Equation, Scc, TermItemKind};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::format_smolstr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{entail, fold, kind};
use crate::error::{ErrorCrumb, ErrorKind};
use crate::source::type_items::collect_unknowns;
use crate::source::{equations, escape, types};
use crate::state::{CheckState, GroupMember, Wanted};
use crate::tree::{self, Evidence, EvidenceBinderId};
use crate::types::{ForallBinder, ForallBinderId, Level, Name, Type, TypeId, UnificationId};

pub fn check_term_items<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for scc in context.grouped.term_scc.iter() {
        let recursive = !matches!(scc, Scc::Base(_));
        check_term_group(state, context, scc.as_slice(), recursive)?;
    }
    Ok(())
}

struct Member<'a> {
    item: TermItemId,
    signature: Option<TypeId>,
    pending: TypeId,
    equations: &'a [Equation],
}

struct Residual {
    constraint: TypeId,
    wanted: Wanted,
}

fn check_term_group<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    items: &[TermItemId],
    recursive: bool,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut values = Vec::new();
    for &item in items {
        match context.lowered.tree.get_term_item_kind(item) {
            Some(TermItemKind::Foreign { signature }) => {
                check_foreign(state, context, item, *signature)?;
            }
            Some(TermItemKind::Value { signature, equations }) => {
                values.push((item, *signature, equations.as_ref()));
            }
            _ => (),
        }
    }
    if values.is_empty() {
        return Ok(());
    }

    let mut members = Vec::with_capacity(values.len());
    for (item, signature, equations) in values {
        let signature = match signature {
            Some(signature) => Some(check_signature(state, context, item, signature)?),
            None => None,
        };
        members.push((item, signature, equations));
    }

    let outer = state.level;
    let (members, declarations) = state.with_level(|state| {
        let members = members.into_iter().map(|(item, signature, equations)| {
            let pending = match signature {
                Some(signature) => {
                    state.checked.term_types.insert(item, signature);
                    signature
                }
                None => {
                    let pending = kind::fresh_type(state, context);
                    state.pending_terms.insert(item, pending);
                    pending
                }
            };
            Member { item, signature, pending, equations }
        });
        let members = members.collect_vec();

        let undefined = if recursive {
            members.iter().map(|member| GroupMember::Term(member.item)).collect_vec()
        } else {
            Vec::new()
        };
        let declarations = state.with_undefined_members(&undefined, |state| {
            let mut declarations = Vec::with_capacity(members.len());
            for member in &members {
                let declaration =
                    state.with_crumb(ErrorCrumb::TermDeclaration(member.item), |state| {
                        equations::check_value(
                            state,
                            context,
                            member.signature,
                            member.pending,
                            member.equations,
                        )
                    })?;
                declarations.push(declaration);
            }
            QueryResult::Ok(declarations)
        })?;

        entail::solve_wanteds(state, context)?;
        QueryResult::Ok((members, declarations))
    })?;

    let mut residuals = collect_residuals(state, context, &members)?;

    let untyped = members.iter().filter(|member| member.signature.is_none());
    let untyped = untyped.map(|member| member.item).collect::<FxHashSet<_>>();

    for (item, residuals) in residuals.iter_mut() {
        let generalisable = mem::take(residuals).into_iter().filter_map(|residual| {
            let head = entail::constraint_head(state, context, residual.constraint);
            let allowed = untyped.contains(item)
                && head.is_some_and(|head| entail::can_be_generalised(state, context, &head));
            if allowed {
                return Some(residual);
            }
            let constraint = residual.constraint;
            state
                .insert_error_at(ErrorKind::NoInstanceFound { constraint }, residual.wanted.crumbs);
            None
        });
        *residuals = generalisable.collect_vec();
    }

    if recursive {
        for (&item, residuals) in residuals.iter_mut() {
            let Some(residual) = residuals.first() else {
                continue;
            };
            let constraint = residual.constraint;
            let crumbs = vec![ErrorCrumb::TermDeclaration(item)];
            state.insert_error_at(
                ErrorKind::CannotGeneralizeRecursiveFunction { constraint },
                crumbs,
            );
            residuals.clear();
        }
    }

    generalise_members(state, context, outer, members, declarations, residuals)
}

fn check_foreign<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TermItemId,
    signature: Option<lowering::TypeId>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let type_id = match signature {
        Some(signature) => check_signature(state, context, item, signature)?,
        None => context.types.unknown,
    };
    state.checked.term_types.insert(item, type_id);
    Ok(())
}

fn check_signature<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TermItemId,
    signature: lowering::TypeId,
) -> QueryResult<TypeId>
where
    Q: ExternalQueries,
{
    state.with_crumb(ErrorCrumb::TermDeclaration(item), |state| {
        let signature = state.with_level(|state| {
            types::check_type(state, context, signature, context.types.type_)
        })?;
        let unknowns = generalisable_unknowns(state, context, &[signature]);
        let binders = quantify_unknowns(state, context, &unknowns);
        let signature = fold::zonk(state, context, signature);
        Ok(context.intern_forall_list(&binders, signature))
    })
}

fn collect_residuals<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    members: &[Member],
) -> QueryResult<FxHashMap<TermItemId, Vec<Residual>>>
where
    Q: ExternalQueries,
{
    loop {
        let mut residuals = FxHashMap::<TermItemId, Vec<Residual>>::default();
        for wanted in mem::take(&mut state.wanted) {
            let constraint = fold::zonk(state, context, wanted.constraint);
            let item = wanted.crumbs.iter().find_map(|crumb| match crumb {
                ErrorCrumb::TermDeclaration(item) => Some(*item),
                _ => None,
            });
            let item = item.or_else(|| members.first().map(|member| member.item));
            let item = item.expect("invariant violated: binding group without members");
            residuals.entry(item).or_default().push(Residual { constraint, wanted });
        }

        let mut improved = false;
        for residuals in residuals.values() {
            let constraints = residuals.iter().map(|residual| residual.constraint).collect_vec();
            improved |= entail::improve_generalised(state, context, &constraints)?;
        }

        if !improved {
            return Ok(residuals);
        }

        state.wanted = residuals.into_values().flatten().map(|residual| residual.wanted).collect();
        entail::solve_wanteds(state, context)?;
    }
}

fn generalise_members<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    outer: Level,
    members: Vec<Member>,
    declarations: Vec<tree::ValueDeclaration>,
    mut residuals: FxHashMap<TermItemId, Vec<Residual>>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut generalised = Vec::with_capacity(members.len());
    for member in &members {
        let residuals = residuals.remove(&member.item).unwrap_or_default();
        let residuals = deduplicate_residuals(state, residuals);
        if member.signature.is_none() {
            check_ambiguity(state, context, member.item, member.pending, &residuals)?;
        }
        generalised.push(residuals);
    }

    let mut types = Vec::new();
    for (member, residuals) in members.iter().zip(&generalised) {
        if member.signature.is_none() {
            types.push(member.pending);
            types.extend(residuals.iter().map(|&(constraint, _)| constraint));
        }
    }
    let unknowns = generalisable_unknowns(state, context, &types);
    let binders = quantify_unknowns(state, context, &unknowns);

    for ((member, declaration), residuals) in members.iter().zip(declarations).zip(generalised) {
        state.pending_terms.remove(&member.item);

        let declaration = match member.signature {
            Some(_) => declaration,
            None => {
                let constraints =
                    residuals.iter().map(|&(constraint, _)| fold::zonk(state, context, constraint));
                let constraints = constraints.collect_vec();
                let inner = fold::zonk(state, context, member.pending);
                let inner = context.intern_constrained_list(&constraints, inner);

                let mut variables = FxHashSet::default();
                collect_variables(context, inner, &mut variables);
                let quantified = binders.iter().copied().filter(|&binder_id| {
                    variables.contains(&context.lookup_forall_binder(binder_id).name)
                });
                let quantified = quantified.collect_vec();
                let type_id = context.intern_forall_list(&quantified, inner);

                let type_binders = quantified
                    .iter()
                    .map(|&binder_id| context.intern_type(Type::Variable(binder_id)));
                let evidence = constraints
                    .into_iter()
                    .zip(&residuals)
                    .map(|(constraint, &(_, binder))| (constraint, binder));
                tree::ValueDeclaration {
                    type_id,
                    type_binders: type_binders.collect::<Arc<[_]>>(),
                    evidence: evidence.collect::<Arc<[_]>>(),
                    parameters: declaration.parameters,
                    equations: declaration.equations,
                }
            }
        };

        let escaped = state.with_crumb(ErrorCrumb::TermDeclaration(member.item), |state| {
            escape::check_value(state, context, &[], &declaration)
        });
        let escaped =
            escaped || check_escape(state, context, outer, member.item, declaration.type_id);
        let declaration = if escaped {
            let type_id = erase_rigids(context, declaration.type_id);
            tree::ValueDeclaration { type_id, ..declaration }
        } else {
            declaration
        };

        state.checked.term_types.insert(member.item, declaration.type_id);
        state.checked.tree.values.insert(member.item, declaration);
    }

    Ok(())
}

fn deduplicate_residuals(
    state: &mut CheckState,
    residuals: Vec<Residual>,
) -> Vec<(TypeId, EvidenceBinderId)> {
    let mut deduplicated: Vec<(TypeId, EvidenceBinderId)> = Vec::new();
    for Residual { constraint, wanted } in residuals {
        let existing = deduplicated.iter().find(|&&(existing, _)| existing == constraint);
        let binder = match existing {
            Some(&(_, binder)) => binder,
            None => {
                let binder = state.fresh_evidence_binder();
                deduplicated.push((constraint, binder));
                binder
            }
        };
        state.checked.tree.set_evidence(wanted.slot, Evidence::Binder(binder));
    }
    deduplicated
}

fn check_ambiguity<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    item: TermItemId,
    type_id: TypeId,
    residuals: &[(TypeId, EvidenceBinderId)],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let mut determined = Vec::new();
    collect_unknowns(state, context, type_id, &mut determined);
    let mut determined = determined.into_iter().collect::<FxHashSet<_>>();

    let mut constraints = Vec::with_capacity(residuals.len());
    for &(constraint, _) in residuals {
        let Some(head) = entail::constraint_head(state, context, constraint) else {
            continue;
        };
        let class = entail::lookup_class(state, context, head.class)?;
        let functional_dependencies = class.map(|class| Arc::clone(&class.functional_dependencies));
        let functional_dependencies = functional_dependencies.unwrap_or_else(|| Arc::from([]));
        let arguments = head.arguments.iter().map(|&argument| {
            let mut unknowns = Vec::new();
            collect_unknowns(state, context, argument, &mut unknowns);
            unknowns
        });
        constraints.push((constraint, functional_dependencies, arguments.collect_vec()));
    }

    loop {
        let mut progress = false;
        for (_, functional_dependencies, arguments) in &constraints {
            for dependency in functional_dependencies.iter() {
                let determiners =
                    dependency.determiners.iter().filter_map(|&index| arguments.get(index));
                let determiners_known =
                    determiners.flatten().all(|unknown| determined.contains(unknown));
                if !determiners_known {
                    continue;
                }
                let determined_arguments =
                    dependency.determined.iter().filter_map(|&index| arguments.get(index));
                for &unknown in determined_arguments.flatten() {
                    progress |= determined.insert(unknown);
                }
            }
        }
        if !progress {
            break;
        }
    }

    for (constraint, _, arguments) in &constraints {
        let ambiguous = arguments.iter().flatten().any(|unknown| !determined.contains(unknown));
        if ambiguous {
            let constraint = fold::zonk(state, context, *constraint);
            let crumbs = vec![ErrorCrumb::TermDeclaration(item)];
            state.insert_error_at(ErrorKind::AmbiguousTypeVariables { constraint }, crumbs);
        }
    }

    Ok(())
}

pub fn generalisable_unknowns<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    types: &[TypeId],
) -> Vec<UnificationId>
where
    Q: ExternalQueries,
{
    let mut unknowns = Vec::new();
    for &type_id in types {
        collect_unknowns(state, context, type_id, &mut unknowns);
    }
    let unknowns = unknowns
        .into_iter()
        .filter(|&unification_id| state.unification(unification_id).level > state.level);
    unknowns.unique().collect_vec()
}

pub fn quantify_unknowns<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    unknowns: &[UnificationId],
) -> Vec<ForallBinderId>
where
    Q: ExternalQueries,
{
    let mut binders = Vec::with_capacity(unknowns.len());
    for &unification_id in unknowns {
        let kind = state.unification(unification_id).kind;
        let kind = fold::zonk(state, context, kind);
        let text = format_smolstr!("t{}", state.unique);
        let text = context.intern_text(text);
        let name = state.fresh_name();
        let binder = ForallBinder { visible: false, name, text, kind };
        let binder_id = context.intern_forall_binder(binder);
        let variable = context.intern_type(Type::Variable(binder_id));
        state.unification_mut(unification_id).solution = Some(variable);
        state.solutions += 1;
        binders.push(binder_id);
    }
    binders
}

fn collect_variables<Q>(context: &CheckContext<Q>, id: TypeId, variables: &mut FxHashSet<Name>)
where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(id).has_variable() {
        return;
    }
    if let Type::Variable(binder_id) = context.lookup_type(id) {
        variables.insert(context.lookup_forall_binder(binder_id).name);
    }
    fold::map_children(context, id, |child| {
        collect_variables(context, child, variables);
        child
    });
}

fn check_escape<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    outer: Level,
    item: TermItemId,
    type_id: TypeId,
) -> bool
where
    Q: ExternalQueries,
{
    let mut rigids = Vec::new();
    collect_rigids(context, type_id, &mut rigids);
    let escaped = rigids.into_iter().find(|&rigid| {
        let Type::Rigid(rigid_id) = context.lookup_type(rigid) else {
            return false;
        };
        context.lookup_rigid(rigid_id).level > outer
    });
    let Some(rigid) = escaped else {
        return false;
    };
    let crumbs = vec![ErrorCrumb::TermDeclaration(item)];
    state.insert_error_at(ErrorKind::EscapedSkolem { rigid, solution: type_id }, crumbs);
    true
}

fn erase_rigids<Q>(context: &CheckContext<Q>, id: TypeId) -> TypeId
where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(id).has_rigid() {
        return id;
    }
    if let Type::Rigid(_) = context.lookup_type(id) {
        return context.types.unknown;
    }
    fold::map_children(context, id, |child| erase_rigids(context, child))
}

fn collect_rigids<Q>(context: &CheckContext<Q>, id: TypeId, rigids: &mut Vec<TypeId>)
where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(id).has_rigid() {
        return;
    }
    if let Type::Rigid(_) = context.lookup_type(id) {
        rigids.push(id);
        return;
    }
    fold::map_children(context, id, |child| {
        collect_rigids(context, child, rigids);
        child
    });
}
