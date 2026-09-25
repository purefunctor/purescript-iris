//! Checks that each skolem introduced by an expected forall appears only in
//! judgments structurally dominated by that forall's typed expression.

use std::sync::Arc;

use building_types::QueryResult;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::CheckContext;
use crate::core::{SkolemScope, Type, TypeId};
use crate::error::{CheckingError, ErrorCrumb, ErrorKind};
use crate::state::CheckState;
use crate::tree::{
    BinderId, BinderKind, CaseAlternative, DeclarationAbstraction, Equation, ExpressionId,
    ExpressionKind, GuardedExpression, InstanceDeclaration, InstanceImplementation, InstanceMember,
    LetBindingChunk, LetBindings, LocalDeclarationId, PatternGuard, RecordBinderField,
    RecordExpressionField, RecordExpressionUpdate, TermDeclaration, TermDeclarationKind,
    WhereExpression,
};
use crate::{CheckedModule, ExternalQueries};

enum Task<'c> {
    Term(&'c TermDeclaration, ErrorCrumb),
    Member(&'c InstanceMember, ErrorCrumb),
    Local(LocalDeclarationId),
    Equation(&'c Equation, ErrorCrumb),
    Guarded(&'c GuardedExpression, ErrorCrumb),
    Where(&'c WhereExpression, ErrorCrumb),
    Bindings(&'c LetBindings, ErrorCrumb),
    Alternative(&'c CaseAlternative, ErrorCrumb),
    Updates(&'c [RecordExpressionUpdate], ErrorCrumb),
    Binder(BinderId, ErrorCrumb),
    Expression(ExpressionId, ErrorCrumb),
    ExitScope(SkolemScope),
    EnterLocalEvidence(LocalDeclarationId),
    ExitEvidence,
}

enum Pass {
    Discover,
    Audit(FxHashSet<SkolemScope>),
}

struct SkolemChecker<'c, 'context, 'queries, Q>
where
    Q: ExternalQueries,
{
    checked: &'c CheckedModule,
    context: &'context CheckContext<'queries, Q>,
    judgments: &'c FxHashSet<ExpressionId>,
    local: Option<LocalDeclarationId>,
    discovering: bool,
    expected_skolems: FxHashSet<SkolemScope>,
    tasks: Vec<Task<'c>>,
    active: FxHashMap<SkolemScope, u32>,
    reported: FxHashSet<SkolemScope>,
    errors: Vec<CheckingError>,
    evidence_depth: u32,
}

pub fn check<Q>(state: &mut CheckState, context: &CheckContext<Q>)
where
    Q: ExternalQueries,
{
    let mut expected_skolems = FxHashSet::default();
    for (_, expression) in state.checked.tree.arena.expressions.iter() {
        let scopes = leading_scopes(context, expression.type_id);
        expected_skolems.extend(scopes);
    }
    if expected_skolems.is_empty() {
        return;
    }

    let (errors, _) = collect_errors(
        &state.checked,
        &state.judgments,
        context,
        None,
        Pass::Audit(expected_skolems),
    );
    state.checked.errors.extend(errors);
}

pub fn check_local<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    declaration: LocalDeclarationId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let type_id = state.checked.tree[declaration].type_id;
    let type_id = super::zonk::zonk(state, context, type_id)?;
    state.checked.tree.set_let_type(declaration, type_id);

    let (_, expected_skolems) = collect_errors(
        &state.checked,
        &state.judgments,
        context,
        Some(declaration),
        Pass::Discover,
    );
    if expected_skolems.is_empty() {
        return Ok(());
    }
    let (errors, _) = collect_errors(
        &state.checked,
        &state.judgments,
        context,
        Some(declaration),
        Pass::Audit(expected_skolems),
    );
    state.checked.errors.extend(errors);
    Ok(())
}

fn new_checker<'c, 'context, 'queries, Q>(
    checked: &'c CheckedModule,
    judgments: &'c FxHashSet<ExpressionId>,
    context: &'context CheckContext<'queries, Q>,
    local: Option<LocalDeclarationId>,
    pass: Pass,
) -> SkolemChecker<'c, 'context, 'queries, Q>
where
    Q: ExternalQueries,
{
    let (discovering, expected_skolems) = match pass {
        Pass::Discover => (true, FxHashSet::default()),
        Pass::Audit(expected_skolems) => (false, expected_skolems),
    };

    let mut tasks = if let Some(declaration) = local {
        vec![Task::Local(declaration)]
    } else {
        let terms = checked.tree.iter_terms();
        let terms = terms.collect_vec();
        let tasks = terms.iter().map(|&(source, declaration)| {
            let crumb = ErrorCrumb::TermDeclaration(source);
            Task::Term(&checked.tree[declaration], crumb)
        });
        let instances = checked.tree.iter_instances().map(|(source, declaration)| {
            let crumb = ErrorCrumb::InstanceDeclaration(source);
            Task::Term(&checked.tree[declaration], crumb)
        });
        let derives = checked.tree.iter_derives().map(|(source, declaration)| {
            let crumb = ErrorCrumb::DeriveDeclaration(source);
            Task::Term(&checked.tree[declaration], crumb)
        });
        tasks.chain(instances).chain(derives).collect_vec()
    };
    tasks.reverse();

    let reported = checked.errors.iter().filter_map(|error| {
        let ErrorKind::EscapedSkolem { skolem, .. } = error.kind else { return None };
        let Type::Rigid(name, _, _) = *context.lookup_type(skolem) else { return None };
        name.scope
    });
    let reported = reported.collect();

    SkolemChecker {
        checked,
        context,
        judgments,
        local,
        discovering,
        expected_skolems,
        tasks,
        active: FxHashMap::default(),
        reported,
        errors: vec![],
        evidence_depth: 0,
    }
}

fn check_term<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    declaration: &'c TermDeclaration,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    inspect_type(checker, declaration.type_id, crumb);

    match &declaration.kind {
        TermDeclarationKind::Value(value) => {
            inspect_abstractions(checker, &value.abstractions, crumb);
            let equations = value.equations.iter().map(|equation| Task::Equation(equation, crumb));
            checker.tasks.extend(equations);
        }
        TermDeclarationKind::Foreign => {}
        TermDeclarationKind::Constructor(constructor) => {
            for &argument in constructor.arguments.iter() {
                inspect_type(checker, argument, crumb);
            }
        }
        TermDeclarationKind::Instance(instance) => check_instance(checker, instance, crumb),
    }
}

fn check_instance<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    instance: &'c InstanceDeclaration,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    for &parameter in instance.rigid_parameters.iter() {
        inspect_rigid_kind(checker, parameter, crumb);
    }
    for evidence in instance.evidences.iter() {
        inspect_type(checker, evidence.constraint, crumb);
    }
    for superclass in instance.superclasses.iter() {
        inspect_type(checker, superclass.constraint, crumb);
    }

    match &instance.implementation {
        InstanceImplementation::Members(members) => {
            let members = members.iter().map(|member| Task::Member(member, crumb));
            checker.tasks.extend(members);
        }
        InstanceImplementation::Delegate { constraint, .. } => {
            inspect_type(checker, *constraint, crumb);
        }
    }
}

fn check_member<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    member: &'c InstanceMember,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    inspect_type(checker, member.implementation_type, crumb);
    inspect_abstractions(checker, &member.abstractions, crumb);
    let equations = member.equations.iter().map(|equation| Task::Equation(equation, crumb));
    checker.tasks.extend(equations);
}

fn check_local_declaration<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    declaration_id: LocalDeclarationId,
) where
    Q: ExternalQueries,
{
    let checked = checker.checked;
    let declaration = &checked.tree[declaration_id];
    let crumb = ErrorCrumb::CheckingLetName(declaration.source);
    if checker.evidence_depth == 0 {
        inspect_type(checker, declaration.type_id, crumb);
        inspect_abstractions(checker, &declaration.value.abstractions, crumb);
    } else {
        inspect_evidence_abstractions(checker, &declaration.value.abstractions, crumb);
    }
    let equations =
        declaration.value.equations.iter().map(|equation| Task::Equation(equation, crumb));
    checker.tasks.extend(equations);
}

fn check_equation<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    equation: &'c Equation,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    checker.tasks.push(Task::Guarded(&equation.guarded_expression, crumb));
    let binders = equation.binders.iter().map(|&binder| Task::Binder(binder, crumb));
    checker.tasks.extend(binders);
}

fn check_guarded<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    guarded: &'c GuardedExpression,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    for alternative in guarded.alternatives.iter() {
        checker.tasks.push(Task::Where(&alternative.where_expression, crumb));
        for guard in alternative.pattern_guards.iter() {
            match guard {
                PatternGuard::Boolean { expression } => {
                    checker.tasks.push(Task::Expression(*expression, crumb));
                }
                PatternGuard::Pattern { binder, expression } => {
                    checker.tasks.push(Task::Binder(*binder, crumb));
                    checker.tasks.push(Task::Expression(*expression, crumb));
                }
            }
        }
    }
}

fn check_where<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    where_expression: &'c WhereExpression,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    checker.tasks.push(Task::Bindings(&where_expression.bindings, crumb));
    checker.tasks.push(Task::Expression(where_expression.expression, crumb));
}

fn check_bindings<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    bindings: &'c LetBindings,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    for chunk in bindings.chunks.iter() {
        match chunk {
            LetBindingChunk::Pattern { binder, where_expression, .. } => {
                checker.tasks.push(Task::Binder(*binder, crumb));
                checker.tasks.push(Task::Where(where_expression, crumb));
            }
            LetBindingChunk::PatternError { where_expression, .. } => {
                if let Some(where_expression) = where_expression {
                    checker.tasks.push(Task::Where(where_expression, crumb));
                }
            }
            LetBindingChunk::Names { declarations, .. } => {
                if checker.evidence_depth > 0 || checker.local.is_none() {
                    let declarations = declarations
                        .iter()
                        .map(|&declaration| Task::EnterLocalEvidence(declaration));
                    checker.tasks.extend(declarations);
                }
            }
        }
    }
}

fn check_alternative<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    alternative: &'c CaseAlternative,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    checker.tasks.push(Task::Guarded(&alternative.guarded_expression, crumb));
    let binders = alternative.binders.iter().map(|&binder| Task::Binder(binder, crumb));
    checker.tasks.extend(binders);
}

fn check_updates<'c, Q>(
    checker: &mut SkolemChecker<'c, '_, '_, Q>,
    updates: &'c [RecordExpressionUpdate],
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    for update in updates {
        match update {
            RecordExpressionUpdate::Error => {}
            RecordExpressionUpdate::Leaf { expression, .. } => {
                checker.tasks.push(Task::Expression(*expression, crumb));
            }
            RecordExpressionUpdate::Branch { updates, .. } => {
                checker.tasks.push(Task::Updates(updates, crumb));
            }
        }
    }
}

fn check_binder<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    binder_id: BinderId,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    if checker.evidence_depth > 0 {
        return;
    }

    let checked = checker.checked;
    let binder = &checked.tree[binder_id];
    inspect_type(checker, binder.type_id, crumb);
    match &binder.kind {
        BinderKind::Error
        | BinderKind::Integer { .. }
        | BinderKind::Number { .. }
        | BinderKind::Variable
        | BinderKind::Wildcard
        | BinderKind::String { .. }
        | BinderKind::Char { .. }
        | BinderKind::Boolean { .. } => {}
        BinderKind::Typed { binder, annotation } => {
            inspect_type(checker, *annotation, crumb);
            checker.tasks.push(Task::Binder(*binder, crumb));
        }
        BinderKind::Named { binder, .. } => {
            checker.tasks.push(Task::Binder(*binder, crumb));
        }
        BinderKind::Array { elements } => {
            let elements = elements.iter().map(|&element| Task::Binder(element, crumb));
            checker.tasks.extend(elements);
        }
        BinderKind::Record { fields } => {
            for field in fields.iter() {
                if let RecordBinderField::Field { binder, .. } = field {
                    checker.tasks.push(Task::Binder(*binder, crumb));
                }
            }
        }
        BinderKind::Constructor { arguments, .. } => {
            let arguments = arguments.iter().map(|&argument| Task::Binder(argument, crumb));
            checker.tasks.extend(arguments);
        }
    }
}

fn check_expression<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    expression_id: ExpressionId,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    let checked = checker.checked;
    let expression = &checked.tree[expression_id];

    for scope in leading_scopes(checker.context, expression.type_id) {
        if checker.discovering {
            checker.expected_skolems.insert(scope);
            checker.reported.insert(scope);
        }
        *checker.active.entry(scope).or_default() += 1;
        checker.tasks.push(Task::ExitScope(scope));
    }

    if checker.evidence_depth == 0 && checker.judgments.contains(&expression_id) {
        inspect_type(checker, expression.type_id, crumb);
    }

    match &expression.kind {
        ExpressionKind::Error
        | ExpressionKind::String { .. }
        | ExpressionKind::Char { .. }
        | ExpressionKind::Boolean { .. }
        | ExpressionKind::Integer { .. }
        | ExpressionKind::Number { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Variable { .. }
        | ExpressionKind::RecordPun { .. } => {}
        ExpressionKind::Section { binder } => {
            checker.tasks.push(Task::Binder(*binder, crumb));
        }
        ExpressionKind::Array { elements } => {
            let elements = elements.iter().map(|&element| Task::Expression(element, crumb));
            checker.tasks.extend(elements);
        }
        ExpressionKind::Record { fields } => {
            for field in fields.iter() {
                let expression = match field {
                    RecordExpressionField::Field { expression, .. }
                    | RecordExpressionField::Pun { expression, .. } => *expression,
                };
                checker.tasks.push(Task::Expression(expression, crumb));
            }
        }
        ExpressionKind::RecordAccess { record, .. } => {
            checker.tasks.push(Task::Expression(*record, crumb));
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            checker.tasks.push(Task::Expression(*record, crumb));
            checker.tasks.push(Task::Updates(updates, crumb));
        }
        ExpressionKind::TermApplication { function, argument } => {
            checker.tasks.push(Task::Expression(*function, crumb));
            checker.tasks.push(Task::Expression(*argument, crumb));
        }
        ExpressionKind::EvidenceApplication { function, constraint, .. } => {
            inspect_type(checker, *constraint, crumb);
            checker.tasks.push(Task::Expression(*function, crumb));
        }
        ExpressionKind::EvidenceAbstraction { binder, expression } => {
            inspect_type(checker, checked.evidence[*binder].constraint, crumb);
            checker.tasks.push(Task::Expression(*expression, crumb));
        }
        ExpressionKind::Lambda { binders, expression } => {
            checker.tasks.push(Task::Expression(*expression, crumb));
            let binders = binders.iter().map(|&binder| Task::Binder(binder, crumb));
            checker.tasks.extend(binders);
        }
        ExpressionKind::IfThenElse { condition, then, else_ } => {
            checker.tasks.push(Task::Expression(*condition, crumb));
            checker.tasks.push(Task::Expression(*then, crumb));
            checker.tasks.push(Task::Expression(*else_, crumb));
        }
        ExpressionKind::Case { scrutinees, alternatives } => {
            let scrutinees = scrutinees.iter().map(|&scrutinee| Task::Expression(scrutinee, crumb));
            checker.tasks.extend(scrutinees);
            let alternatives =
                alternatives.iter().map(|alternative| Task::Alternative(alternative, crumb));
            checker.tasks.extend(alternatives);
        }
        ExpressionKind::Let { bindings, expression } => {
            checker.tasks.push(Task::Bindings(bindings, crumb));
            checker.tasks.push(Task::Expression(*expression, crumb));
        }
    }
}

fn exit_scope<Q>(checker: &mut SkolemChecker<'_, '_, '_, Q>, scope: SkolemScope)
where
    Q: ExternalQueries,
{
    let count =
        checker.active.get_mut(&scope).expect("invariant violated: exited inactive skolem scope");
    *count -= 1;
    if *count == 0 {
        checker.active.remove(&scope);
    }
}

fn enter_local_evidence<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    declaration: LocalDeclarationId,
) where
    Q: ExternalQueries,
{
    checker.evidence_depth += 1;
    checker.tasks.push(Task::ExitEvidence);
    checker.tasks.push(Task::Local(declaration));
}

fn exit_evidence<Q>(checker: &mut SkolemChecker<'_, '_, '_, Q>)
where
    Q: ExternalQueries,
{
    checker.evidence_depth -= 1;
}

fn inspect_abstractions<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    abstractions: &[DeclarationAbstraction],
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    for abstraction in abstractions {
        match abstraction {
            DeclarationAbstraction::Type { binder, rigid } => {
                let binder = checker.context.lookup_forall_binder(*binder);
                inspect_type(checker, binder.kind, crumb);
                inspect_rigid_kind(checker, *rigid, crumb);
            }
            DeclarationAbstraction::Evidence { constraint, .. } => {
                inspect_type(checker, *constraint, crumb);
            }
            DeclarationAbstraction::Argument => {}
        }
    }
}

fn inspect_evidence_abstractions<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    abstractions: &[DeclarationAbstraction],
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    for abstraction in abstractions {
        if let DeclarationAbstraction::Evidence { constraint, .. } = abstraction {
            inspect_type(checker, *constraint, crumb);
        }
    }
}

fn inspect_rigid_kind<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    rigid: TypeId,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    if let Type::Rigid(_, _, kind) = *checker.context.lookup_type(rigid) {
        inspect_type(checker, kind, crumb);
    }
}

fn inspect_type<Q>(
    checker: &mut SkolemChecker<'_, '_, '_, Q>,
    annotation: TypeId,
    crumb: ErrorCrumb,
) where
    Q: ExternalQueries,
{
    // Unification variables are not looked through, so types without rigid
    // variables cannot contain an escaped skolem.
    let has_rigid = |type_id| checker.context.lookup_type_flags(type_id).has_rigid();

    if !has_rigid(annotation) {
        return;
    }

    let mut pending = vec![annotation];
    let mut visited = FxHashSet::default();

    while let Some(type_id) = pending.pop() {
        if !has_rigid(type_id) || !visited.insert(type_id) {
            continue;
        }

        match *checker.context.lookup_type(type_id) {
            Type::Application(function, argument)
            | Type::KindApplication(function, argument)
            | Type::Constrained(function, argument)
            | Type::Function(function, argument)
            | Type::Kinded(function, argument) => {
                pending.push(function);
                pending.push(argument);
            }
            Type::Forall(binder, inner) => {
                let binder = checker.context.lookup_forall_binder(binder);
                pending.push(binder.kind);
                pending.push(inner);
            }
            Type::Row(row) => {
                let row = checker.context.lookup_row_type(row);
                let fields = row.fields.iter().map(|field| field.id);
                pending.extend(fields);
                pending.extend(row.tail);
            }
            Type::Rigid(name, _, kind) => {
                if let Some(scope) = name.scope
                    && checker.expected_skolems.contains(&scope)
                    && !checker.active.contains_key(&scope)
                    && checker.reported.insert(scope)
                {
                    let skolem = type_id;
                    let kind = ErrorKind::EscapedSkolem { skolem, type_id: annotation };
                    checker.errors.push(CheckingError { kind, crumbs: Arc::from([crumb]) });
                }
                pending.push(kind);
            }
            Type::Constructor(_, _)
            | Type::Integer(_)
            | Type::String(_, _)
            | Type::Unification(_)
            | Type::Free(_)
            | Type::Unknown(_) => {}
        }
    }
}

fn collect_errors<'c, Q>(
    checked: &'c CheckedModule,
    judgments: &'c FxHashSet<ExpressionId>,
    context: &CheckContext<Q>,
    local: Option<LocalDeclarationId>,
    pass: Pass,
) -> (Vec<CheckingError>, FxHashSet<SkolemScope>)
where
    Q: ExternalQueries,
{
    let mut checker = new_checker(checked, judgments, context, local, pass);

    while let Some(task) = checker.tasks.pop() {
        match task {
            Task::Term(declaration, crumb) => check_term(&mut checker, declaration, crumb),
            Task::Member(member, crumb) => check_member(&mut checker, member, crumb),
            Task::Local(declaration) => check_local_declaration(&mut checker, declaration),
            Task::Equation(equation, crumb) => check_equation(&mut checker, equation, crumb),
            Task::Guarded(guarded, crumb) => check_guarded(&mut checker, guarded, crumb),
            Task::Where(where_expression, crumb) => {
                check_where(&mut checker, where_expression, crumb);
            }
            Task::Bindings(bindings, crumb) => check_bindings(&mut checker, bindings, crumb),
            Task::Alternative(alternative, crumb) => {
                check_alternative(&mut checker, alternative, crumb);
            }
            Task::Updates(updates, crumb) => check_updates(&mut checker, updates, crumb),
            Task::Binder(binder, crumb) => check_binder(&mut checker, binder, crumb),
            Task::Expression(expression, crumb) => {
                check_expression(&mut checker, expression, crumb);
            }
            Task::ExitScope(scope) => exit_scope(&mut checker, scope),
            Task::EnterLocalEvidence(declaration) => {
                enter_local_evidence(&mut checker, declaration);
            }
            Task::ExitEvidence => exit_evidence(&mut checker),
        }
    }

    (checker.errors, checker.expected_skolems)
}

fn leading_scopes<Q>(context: &CheckContext<Q>, mut type_id: TypeId) -> Vec<SkolemScope>
where
    Q: ExternalQueries,
{
    let mut scopes = vec![];
    while let Type::Forall(binder, inner) = *context.lookup_type(type_id) {
        let binder = context.lookup_forall_binder(binder);
        let Some(scope) = binder.scope else { break };
        scopes.push(scope);
        type_id = inner;
    }
    scopes
}
