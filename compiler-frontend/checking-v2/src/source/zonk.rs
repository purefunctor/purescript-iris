use std::sync::Arc;

use smol_str::SmolStr;

use crate::context::CheckContext;
use crate::core::fold;
use crate::state::CheckState;
use crate::tree::{
    Abstraction, CaseBranch, CheckedTree, Equation, Evidence, EvidenceBinderId, ExpressionKind,
    Guard, Guarded, GuardedExpression, LocalBinding, LocalValue, Obligation, Parameter,
    ValueDeclaration,
};
use crate::types::{ForallBinderId, TypeId};
use crate::{CheckedConstructor, CheckedModule, ExternalQueries};

pub fn zonk_tree<Q>(state: &CheckState, context: &CheckContext<Q>, tree: &mut CheckedTree)
where
    Q: ExternalQueries,
{
    for expression in tree.expressions.values_mut() {
        expression.type_id = fold::zonk(state, context, expression.type_id);
        match &mut expression.kind {
            ExpressionKind::Instantiate { type_arguments, .. } => {
                *type_arguments = zonk_types(state, context, type_arguments);
            }
            ExpressionKind::Abstract { type_binders, evidence, .. } => {
                *type_binders = zonk_types(state, context, type_binders);
                *evidence = zonk_evidence_binders(state, context, evidence);
            }
            ExpressionKind::Let { bindings, .. } => {
                *bindings = zonk_bindings(state, context, bindings);
            }
            ExpressionKind::Case { branches, .. } => {
                let zonked = branches.iter().map(|branch| CaseBranch {
                    binders: Arc::clone(&branch.binders),
                    guarded: zonk_guarded(state, context, &branch.guarded),
                });
                *branches = zonked.collect::<Arc<[_]>>();
            }
            _ => (),
        }
    }

    for binder in tree.binders.values_mut() {
        binder.type_id = fold::zonk(state, context, binder.type_id);
    }

    for evidence in tree.evidence.iter_mut() {
        *evidence = zonk_evidence(state, context, evidence);
    }

    for value in tree.values.values_mut() {
        *value = zonk_value(state, context, value);
    }

    for instance in tree.instances.values_mut() {
        instance.evidence = zonk_evidence_binders(state, context, &instance.evidence);
        let members = instance.members.iter().map(|member| {
            let value = zonk_value(state, context, &member.value);
            crate::tree::InstanceMember { item: member.item, value }
        });
        instance.members = members.collect::<Arc<[_]>>();
        let obligations = instance.obligations.iter().map(|obligation| Obligation {
            constraint: fold::zonk(state, context, obligation.constraint),
            abstraction: obligation
                .abstraction
                .as_ref()
                .map(|abstraction| zonk_abstraction(state, context, abstraction)),
            evidence: obligation.evidence,
        });
        instance.obligations = obligations.collect::<Arc<[_]>>();
    }
}

pub fn zonk_declarations<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    checked: &mut CheckedModule,
) where
    Q: ExternalQueries,
{
    let exported = |id: TypeId| fold::zonk_exported(state, context, id);
    let exported_list = |ids: &[TypeId]| ids.iter().map(|&id| exported(id)).collect::<Arc<[_]>>();
    let exported_binders = |binders: &[ForallBinderId]| {
        let binders =
            binders.iter().map(|&binder_id| fold::zonk_exported_binder(state, context, binder_id));
        binders.collect::<Arc<[_]>>()
    };

    for kind in checked.type_kinds.values_mut() {
        *kind = exported(*kind);
    }

    for type_id in checked.term_types.values_mut() {
        *type_id = exported(*type_id);
    }

    for synonym in checked.synonyms.values_mut() {
        synonym.kind_binders = exported_binders(&synonym.kind_binders);
        synonym.parameters = exported_binders(&synonym.parameters);
        synonym.body = exported(synonym.body);
    }

    for data in checked.data.values_mut() {
        data.kind_binders = exported_binders(&data.kind_binders);
        data.parameters = exported_binders(&data.parameters);
        let constructors = data.constructors.iter().map(|constructor| CheckedConstructor {
            item: constructor.item,
            fields: exported_list(&constructor.fields),
        });
        data.constructors = constructors.collect();
    }

    for class in checked.classes.values_mut() {
        class.kind_binders = exported_binders(&class.kind_binders);
        class.parameters = exported_binders(&class.parameters);
        class.superclasses = exported_list(&class.superclasses);
        let members =
            class.members.iter().map(|&(member, member_type)| (member, exported(member_type)));
        class.members = members.collect();
    }

    for instance in checked.instances.iter_mut() {
        instance.binders = exported_binders(&instance.binders);
        instance.constraints = exported_list(&instance.constraints);
        instance.kind_arguments = exported_list(&instance.kind_arguments);
        instance.arguments = exported_list(&instance.arguments);
    }
}

fn zonk_types<Q>(state: &CheckState, context: &CheckContext<Q>, types: &[TypeId]) -> Arc<[TypeId]>
where
    Q: ExternalQueries,
{
    types.iter().map(|&type_id| fold::zonk(state, context, type_id)).collect()
}

fn zonk_evidence_binders<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    evidence: &[(TypeId, EvidenceBinderId)],
) -> Arc<[(TypeId, EvidenceBinderId)]>
where
    Q: ExternalQueries,
{
    let evidence = evidence
        .iter()
        .map(|&(constraint, binder)| (fold::zonk(state, context, constraint), binder));
    evidence.collect()
}

fn zonk_evidence<Q>(state: &CheckState, context: &CheckContext<Q>, evidence: &Evidence) -> Evidence
where
    Q: ExternalQueries,
{
    match evidence {
        Evidence::Instance { file_id, origin, type_arguments, arguments } => Evidence::Instance {
            file_id: *file_id,
            origin: *origin,
            type_arguments: zonk_types(state, context, type_arguments),
            arguments: Arc::clone(arguments),
        },
        Evidence::Superclass { from, class, index } => Evidence::Superclass {
            from: Arc::new(zonk_evidence(state, context, from)),
            class: *class,
            index: *index,
        },
        other => Evidence::clone(other),
    }
}

fn zonk_value<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    value: &ValueDeclaration,
) -> ValueDeclaration
where
    Q: ExternalQueries,
{
    let equations = value.equations.iter().map(|equation| Equation {
        binders: Arc::clone(&equation.binders),
        guarded: zonk_guarded(state, context, &equation.guarded),
    });

    let parameters = value.parameters.iter().map(|parameter| match parameter {
        Parameter::Argument(type_id) => Parameter::Argument(fold::zonk(state, context, *type_id)),
        Parameter::Abstract(abstraction) => {
            Parameter::Abstract(zonk_abstraction(state, context, abstraction))
        }
    });

    ValueDeclaration {
        type_id: fold::zonk(state, context, value.type_id),
        type_binders: zonk_types(state, context, &value.type_binders),
        evidence: zonk_evidence_binders(state, context, &value.evidence),
        parameters: parameters.collect(),
        equations: equations.collect(),
    }
}

fn zonk_abstraction<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    abstraction: &Abstraction,
) -> Abstraction
where
    Q: ExternalQueries,
{
    Abstraction {
        type_binders: zonk_types(state, context, &abstraction.type_binders),
        evidence: zonk_evidence_binders(state, context, &abstraction.evidence),
    }
}

fn zonk_guarded<Q>(state: &CheckState, context: &CheckContext<Q>, guarded: &Guarded) -> Guarded
where
    Q: ExternalQueries,
{
    let Guarded::Conditionals(conditionals) = guarded else {
        return Guarded::clone(guarded);
    };

    let conditionals = conditionals.iter().map(|conditional| {
        let guards = conditional.guards.iter().map(|guard| match guard {
            Guard::Let(bindings) => Guard::Let(zonk_bindings(state, context, bindings)),
            other => Guard::clone(other),
        });
        GuardedExpression { guards: guards.collect(), expression: conditional.expression }
    });

    Guarded::Conditionals(conditionals.collect())
}

fn zonk_bindings<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    bindings: &[LocalBinding],
) -> Arc<[LocalBinding]>
where
    Q: ExternalQueries,
{
    let bindings = bindings.iter().map(|binding| match binding {
        LocalBinding::Value(local) => LocalBinding::Value(LocalValue {
            source: local.source,
            name: SmolStr::clone(&local.name),
            value: zonk_value(state, context, &local.value),
        }),
        other => LocalBinding::clone(other),
    });
    bindings.collect()
}
