use std::sync::Arc;

use building_types::QueryResult;
use checking::evidence::{
    Evidence, EvidenceBinderId, EvidenceId, EvidenceState, EvidenceVarId, InstanceCandidateOrigin,
};
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};

use crate::error::UnsupportedState;
use crate::optimize::{expression_globals, reachable_expressions};
use crate::tree::{
    Binding, Declaration, DeclarationKind, Expression, ExpressionId, ExpressionKind, Global,
    GlobalId, InstanceIdentity, Parameter, ReflectableEvidence, ReflectableOrdering, Storage,
    SynthesizedEvidence,
};

use super::{BindingSource, Context, ConversionResult, lowercase_initial};

const MAX_EVIDENCE_NAME_FRAGMENTS: usize = 4;
const MAX_INLINE_EVIDENCE_DEPTH: usize = 32;

#[derive(Default)]
pub(super) struct EvidenceScope {
    // Evidence containing local dictionary parameters cannot escape its lexical scope.
    constructions: FxHashMap<EvidenceKey, EvidenceConstruction>,
    bindings: Vec<EvidenceBinding>,
}

#[derive(Default)]
pub(super) struct EvidenceHoisting {
    // Closed evidence is collected across lexical scopes so repetition can introduce a module global.
    occurrences: FxHashMap<ClosedEvidenceKey, EvidenceOccurrences>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ClosedEvidenceKey {
    Dictionary(EvidenceKey),
    Member { member: (files::FileId, indexing::TermItemId), evidence: EvidenceKey },
}

#[derive(Default)]
struct EvidenceOccurrences {
    expressions: Vec<ExpressionId>,
    constraint_name: Option<SmolStr>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct EvidenceKey(u32);

#[derive(Clone, PartialEq, Eq, Hash)]
enum EvidenceKeyKind {
    Given(EvidenceBinderId),
    Instance { origin: InstanceCandidateOrigin, subgoals: Vec<EvidenceKey> },
    Superclass { parent: EvidenceKey, superclass: checking::evidence::SuperclassId },
    Opaque(EvidenceId),
    InvalidEvidence(EvidenceId),
    InvalidVariable(EvidenceVarId),
}

struct EvidenceKeyData {
    kind: EvidenceKeyKind,
    closed: bool,
    dependency_order: usize,
    inline_height: usize,
}

#[derive(Default)]
pub(super) struct EvidenceKeys {
    keys: Vec<EvidenceKeyData>,
    interned: FxHashMap<EvidenceKeyKind, EvidenceKey>,
    evidence: FxHashMap<EvidenceId, EvidenceKey>,
}

enum EvidenceKeyStep {
    Enter(EvidenceId),
    InvalidVariable(EvidenceVarId),
    FinishVariable(EvidenceId),
    FinishInstance { evidence: EvidenceId, origin: InstanceCandidateOrigin, subgoals: usize },
    FinishSuperclass { evidence: EvidenceId, superclass: checking::evidence::SuperclassId },
}

impl EvidenceKeys {
    fn key(&mut self, evidences: &checking::evidence::Evidences, root: EvidenceId) -> EvidenceKey {
        if let Some(&key) = self.evidence.get(&root) {
            return key;
        }

        let mut steps = vec![EvidenceKeyStep::Enter(root)];
        let mut results = vec![];
        let mut visiting = FxHashSet::default();

        while let Some(step) = steps.pop() {
            match step {
                EvidenceKeyStep::Enter(evidence) => {
                    if let Some(&key) = self.evidence.get(&evidence) {
                        results.push(key);
                        continue;
                    }
                    if !visiting.insert(evidence) {
                        results.push(self.intern(EvidenceKeyKind::InvalidEvidence(evidence)));
                        continue;
                    }

                    match &evidences[evidence] {
                        Evidence::Variable(variable) => {
                            let EvidenceState::Solved(child) = evidences[*variable].state else {
                                let key = self.intern(EvidenceKeyKind::InvalidVariable(*variable));
                                self.evidence.insert(evidence, key);
                                visiting.remove(&evidence);
                                results.push(key);
                                continue;
                            };
                            steps.push(EvidenceKeyStep::FinishVariable(evidence));
                            steps.push(EvidenceKeyStep::Enter(child));
                        }
                        Evidence::Given(binder) => {
                            let key = self.intern(EvidenceKeyKind::Given(*binder));
                            self.evidence.insert(evidence, key);
                            visiting.remove(&evidence);
                            results.push(key);
                        }
                        Evidence::Instance { origin, subgoals } => {
                            steps.push(EvidenceKeyStep::FinishInstance {
                                evidence,
                                origin: *origin,
                                subgoals: subgoals.len(),
                            });
                            steps.extend(subgoals.iter().rev().map(|subgoal| {
                                match evidences[*subgoal].state {
                                    EvidenceState::Solved(evidence) => {
                                        EvidenceKeyStep::Enter(evidence)
                                    }
                                    EvidenceState::Unsolved | EvidenceState::Error => {
                                        EvidenceKeyStep::InvalidVariable(*subgoal)
                                    }
                                }
                            }));
                        }
                        Evidence::Superclass { parent, superclass } => {
                            steps.push(EvidenceKeyStep::FinishSuperclass {
                                evidence,
                                superclass: *superclass,
                            });
                            steps.push(EvidenceKeyStep::Enter(*parent));
                        }
                        Evidence::Trivial | Evidence::Synthesized(_) => {
                            let key = self.intern(EvidenceKeyKind::Opaque(evidence));
                            self.evidence.insert(evidence, key);
                            visiting.remove(&evidence);
                            results.push(key);
                        }
                    }
                }
                EvidenceKeyStep::InvalidVariable(variable) => {
                    results.push(self.intern(EvidenceKeyKind::InvalidVariable(variable)));
                }
                EvidenceKeyStep::FinishVariable(evidence) => {
                    let key = results
                        .pop()
                        .expect("invariant violated: evidence variable has no child key");
                    self.evidence.insert(evidence, key);
                    visiting.remove(&evidence);
                    results.push(key);
                }
                EvidenceKeyStep::FinishInstance { evidence, origin, subgoals } => {
                    let first = results.len() - subgoals;
                    let subgoals = results.drain(first..).collect();
                    let key = self.intern(EvidenceKeyKind::Instance { origin, subgoals });
                    self.evidence.insert(evidence, key);
                    visiting.remove(&evidence);
                    results.push(key);
                }
                EvidenceKeyStep::FinishSuperclass { evidence, superclass } => {
                    let parent = results
                        .pop()
                        .expect("invariant violated: superclass evidence has no parent key");
                    let key = self.intern(EvidenceKeyKind::Superclass { parent, superclass });
                    self.evidence.insert(evidence, key);
                    visiting.remove(&evidence);
                    results.push(key);
                }
            }
        }

        let key = results.pop().expect("invariant violated: evidence has no key");
        self.evidence.insert(root, key);
        key
    }

    fn intern(&mut self, kind: EvidenceKeyKind) -> EvidenceKey {
        if let Some(&key) = self.interned.get(&kind) {
            return key;
        }

        let (closed, dependency_order, inline_height) = match &kind {
            EvidenceKeyKind::Given(_) => (false, 0, 0),
            EvidenceKeyKind::Opaque(_) => (true, 0, 0),
            EvidenceKeyKind::InvalidEvidence(_) | EvidenceKeyKind::InvalidVariable(_) => {
                (false, 0, 0)
            }
            EvidenceKeyKind::Instance { subgoals, .. } => {
                let closed = subgoals.iter().all(|key| self.data(*key).closed);
                let dependency_order = subgoals
                    .iter()
                    .map(|key| self.data(*key).dependency_order)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                let inline_height = subgoals
                    .iter()
                    .map(|key| self.data(*key).inline_height)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
                    % MAX_INLINE_EVIDENCE_DEPTH;
                (closed, dependency_order, inline_height)
            }
            EvidenceKeyKind::Superclass { parent, .. } => {
                let parent = self.data(*parent);
                let dependency_order = parent.dependency_order.saturating_add(1);
                let inline_height =
                    parent.inline_height.saturating_add(1) % MAX_INLINE_EVIDENCE_DEPTH;
                (parent.closed, dependency_order, inline_height)
            }
        };
        let key = EvidenceKey(self.keys.len() as u32);
        self.keys.push(EvidenceKeyData {
            kind: EvidenceKeyKind::clone(&kind),
            closed,
            dependency_order,
            inline_height,
        });
        self.interned.insert(kind, key);
        key
    }

    fn data(&self, key: EvidenceKey) -> &EvidenceKeyData {
        &self.keys[key.0 as usize]
    }

    fn kind(&self, key: EvidenceKey) -> &EvidenceKeyKind {
        &self.data(key).kind
    }

    fn is_closed(&self, key: EvidenceKey) -> bool {
        self.data(key).closed
    }

    fn dependency_order(&self, key: EvidenceKey) -> usize {
        self.data(key).dependency_order
    }

    fn is_self_contained(&self, key: EvidenceKey) -> bool {
        self.dependency_order(key) < MAX_INLINE_EVIDENCE_DEPTH
    }

    fn is_cut_point(&self, key: EvidenceKey) -> bool {
        let data = self.data(key);
        data.dependency_order != 0 && data.inline_height == 0
    }
}

impl ClosedEvidenceKey {
    fn dependency_order(&self, keys: &EvidenceKeys) -> usize {
        match self {
            ClosedEvidenceKey::Dictionary(evidence) => keys.dependency_order(*evidence),
            ClosedEvidenceKey::Member { evidence, .. } => {
                keys.dependency_order(*evidence).saturating_add(1)
            }
        }
    }

    fn contains_unsafe_instance(
        &self,
        keys: &EvidenceKeys,
        unsafe_instances: &FxHashSet<InstanceIdentity>,
    ) -> bool {
        let evidence = match self {
            ClosedEvidenceKey::Dictionary(evidence)
            | ClosedEvidenceKey::Member { evidence, .. } => evidence,
        };
        evidence_contains_unsafe_instance(keys, *evidence, unsafe_instances)
    }
}

impl EvidenceHoisting {
    fn record(
        &mut self,
        key: ClosedEvidenceKey,
        expression: ExpressionId,
        constraint_name: Option<SmolStr>,
    ) {
        let occurrences = self.occurrences.entry(key).or_default();
        occurrences.expressions.push(expression);

        if occurrences.constraint_name.is_none() {
            occurrences.constraint_name = constraint_name;
        }
    }
}

#[derive(Clone)]
enum EvidenceConstruction {
    Inline { expression: ExpressionId, name: SmolStr },
    Shared(Parameter),
}

struct EvidenceBinding {
    evidence: EvidenceKey,
    binding: Binding,
}

#[derive(Default)]
struct EvidenceOccurrence {
    constructions: FxHashMap<EvidenceKey, OccurrenceConstruction>,
    bindings: Vec<OccurrenceBinding>,
    next_order: usize,
}

#[derive(Clone)]
enum OccurrenceConstruction {
    Inline { expression: ExpressionId, name: SmolStr, order: usize },
    Shared(Parameter),
}

struct OccurrenceBinding {
    order: usize,
    binding: Binding,
}

enum EvidenceConversionStep {
    Variable {
        variable: EvidenceVarId,
        constraint: Option<checking::TypeId>,
    },
    Evidence {
        evidence: EvidenceId,
        constraint: Option<checking::TypeId>,
    },
    FinishVariable(EvidenceVarId),
    FinishInstance {
        evidence: EvidenceKey,
        function: ExpressionId,
        name: SmolStr,
        arguments: usize,
        constraint: Option<checking::TypeId>,
    },
    FinishSuperclass {
        evidence: EvidenceKey,
        superclass: checking::evidence::SuperclassId,
        constraint: Option<checking::TypeId>,
    },
}

pub(super) fn evidence_variable(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    variable: EvidenceVarId,
    constraint: Option<checking::TypeId>,
) -> ConversionResult<ExpressionId> {
    let mut steps = vec![EvidenceConversionStep::Variable { variable, constraint }];
    let mut expressions = vec![];
    let mut lowering = FxHashSet::default();
    let mut occurrence = EvidenceOccurrence::default();

    while let Some(step) = steps.pop() {
        match step {
            EvidenceConversionStep::Variable { variable, constraint } => {
                if !lowering.insert(variable) {
                    return Err(context.unsupported(UnsupportedState::CyclicEvidence(variable)));
                }
                match context.checked.evidence[variable].state {
                    EvidenceState::Unsolved => {
                        return Err(
                            context.unsupported(UnsupportedState::UnsolvedEvidence(variable))
                        );
                    }
                    EvidenceState::Solved(evidence) => {
                        steps.push(EvidenceConversionStep::FinishVariable(variable));
                        steps.push(EvidenceConversionStep::Evidence { evidence, constraint });
                    }
                    EvidenceState::Error => {
                        lowering.remove(&variable);
                        expressions.push(context.expression(ExpressionKind::Error));
                    }
                }
            }
            EvidenceConversionStep::Evidence { evidence, constraint } => {
                let checked = Arc::clone(&context.checked);
                match &checked.evidence[evidence] {
                    Evidence::Variable(variable) => {
                        steps.push(EvidenceConversionStep::Variable {
                            variable: *variable,
                            constraint,
                        });
                    }
                    Evidence::Given(binder) => {
                        let parameter = context.evidence_parameter(*binder)?;
                        expressions.push(context.expression(ExpressionKind::Local { parameter }));
                    }
                    Evidence::Instance { origin, subgoals } => {
                        let evidence = context.evidence_key(evidence);
                        if let Some(expression) =
                            context.shared_evidence(&mut occurrence, evidence)?
                        {
                            expressions.push(expression);
                            continue;
                        }
                        let global = context.instance_global(*origin)?;
                        let name = format_smolstr!("{}Dict", global.item_name);
                        let function = context.expression(ExpressionKind::Global { global });
                        steps.push(EvidenceConversionStep::FinishInstance {
                            evidence,
                            function,
                            name,
                            arguments: subgoals.len(),
                            constraint,
                        });
                        steps.extend(subgoals.iter().rev().map(|variable| {
                            EvidenceConversionStep::Variable {
                                variable: *variable,
                                constraint: None,
                            }
                        }));
                    }
                    Evidence::Superclass { parent, superclass } => {
                        let evidence = context.evidence_key(evidence);
                        if let Some(expression) =
                            context.shared_evidence(&mut occurrence, evidence)?
                        {
                            expressions.push(expression);
                            continue;
                        }
                        steps.push(EvidenceConversionStep::FinishSuperclass {
                            evidence,
                            superclass: *superclass,
                            constraint,
                        });
                        steps.push(EvidenceConversionStep::Evidence {
                            evidence: *parent,
                            constraint: None,
                        });
                    }
                    Evidence::Trivial => {
                        expressions.push(context.expression(ExpressionKind::TrivialEvidence));
                    }
                    Evidence::Synthesized(evidence) => {
                        let evidence = synthesized_evidence(context, evidence);
                        expressions.push(
                            context.expression(ExpressionKind::SynthesizedEvidence { evidence }),
                        );
                    }
                }
            }
            EvidenceConversionStep::FinishVariable(variable) => {
                lowering.remove(&variable);
            }
            EvidenceConversionStep::FinishInstance {
                evidence,
                function,
                name,
                arguments,
                constraint,
            } => {
                let first = expressions.len() - arguments;
                let arguments = expressions.drain(first..).collect::<Vec<_>>();
                let has_arguments = !arguments.is_empty();
                let construction = context.synthetic_application(function, arguments)?;
                let expression = if has_arguments {
                    context.record_evidence(
                        &mut occurrence,
                        evidence,
                        construction,
                        name,
                        constraint,
                    )?
                } else {
                    construction
                };
                expressions.push(expression);
            }
            EvidenceConversionStep::FinishSuperclass { evidence, superclass, constraint } => {
                let record = expressions
                    .pop()
                    .expect("invariant violated: superclass evidence has no parent expression");
                let field = context.superclass_field(superclass)?;
                let name = format_smolstr!("{}Dict", field.name);
                let accessor = context.expression(ExpressionKind::Project { record, field });
                let construction = context.expression(ExpressionKind::Application {
                    function: accessor,
                    arguments: Arc::from([]),
                    synthetic: true,
                });
                let expression = context.record_evidence(
                    &mut occurrence,
                    evidence,
                    construction,
                    name,
                    constraint,
                )?;
                expressions.push(expression);
            }
        }
    }

    let expression =
        expressions.pop().expect("invariant violated: evidence conversion produced no expression");
    debug_assert!(expressions.is_empty());
    Ok(bind_evidence_occurrence(context, occurrence, expression))
}

fn bind_evidence_occurrence(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    mut occurrence: EvidenceOccurrence,
    body: ExpressionId,
) -> ExpressionId {
    if occurrence.bindings.is_empty() {
        return body;
    }
    occurrence.bindings.sort_by_key(|binding| binding.order);
    let bindings = occurrence.bindings.into_iter().map(|binding| binding.binding);
    context.expression(ExpressionKind::Let { recursive: false, bindings: bindings.collect(), body })
}

fn synthesized_evidence(
    _context: &Context<'_, impl checking::ExternalQueries>,
    evidence: &checking::evidence::SynthesizedEvidence,
) -> SynthesizedEvidence {
    match evidence {
        checking::evidence::SynthesizedEvidence::IsSymbol(symbol) => {
            SynthesizedEvidence::IsSymbol(symbol.clone())
        }
        checking::evidence::SynthesizedEvidence::Reflectable(reflectable) => {
            let reflectable = match reflectable {
                checking::evidence::ReflectableEvidence::Integer(value) => {
                    ReflectableEvidence::Integer(*value)
                }
                checking::evidence::ReflectableEvidence::String(value) => {
                    ReflectableEvidence::String(value.clone())
                }
                checking::evidence::ReflectableEvidence::Boolean(value) => {
                    ReflectableEvidence::Boolean(*value)
                }
                checking::evidence::ReflectableEvidence::Ordering(ordering) => {
                    let ordering = match ordering {
                        checking::evidence::ReflectableOrdering::Less => ReflectableOrdering::Less,
                        checking::evidence::ReflectableOrdering::Equal => {
                            ReflectableOrdering::Equal
                        }
                        checking::evidence::ReflectableOrdering::Greater => {
                            ReflectableOrdering::Greater
                        }
                    };
                    ReflectableEvidence::Ordering(ordering)
                }
            };
            SynthesizedEvidence::Reflectable(reflectable)
        }
    }
}

fn order_evidence_bindings(
    keys: &EvidenceKeys,
    mut bindings: Vec<EvidenceBinding>,
) -> Vec<Binding> {
    // Every prerequisite precedes the evidence that consumes it, so this order places inputs
    // before the non-recursive bindings that reference them.
    bindings.sort_by_key(|binding| keys.dependency_order(binding.evidence));
    let bindings = bindings.into_iter().map(|binding| binding.binding);
    bindings.collect()
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn evidence_scope(
        &mut self,
        convert: impl FnOnce(&mut Context<'c, Q>) -> ConversionResult<ExpressionId>,
    ) -> ConversionResult<ExpressionId> {
        self.evidence_scopes.push(EvidenceScope::default());
        let result = convert(self);
        let scope = self
            .evidence_scopes
            .pop()
            .expect("invariant violated: evidence scope disappeared during conversion");
        let body = result?;
        if scope.bindings.is_empty() {
            return Ok(body);
        }
        let bindings = order_evidence_bindings(&self.evidence_keys, scope.bindings);
        Ok(self.expression(ExpressionKind::Let {
            recursive: false,
            bindings: bindings.into(),
            body,
        }))
    }

    pub(super) fn hoist_closed_evidence(
        &mut self,
        declarations: &mut Vec<Declaration>,
    ) -> ConversionResult<()> {
        let mut occurrences = std::mem::take(&mut self.evidence_hoisting.occurrences);
        // Reachability and deduplication only remove occurrences, so evidence recorded fewer
        // than twice can never be shared and needs neither module-wide analysis.
        occurrences.retain(|_, occurrences| occurrences.expressions.len() >= 2);
        if occurrences.is_empty() {
            return Ok(());
        }

        let roots = declarations.iter().filter_map(|declaration| match declaration.kind {
            DeclarationKind::Value(expression) => Some(expression),
            DeclarationKind::Constructor { .. } | DeclarationKind::Foreign => None,
        });
        let reachable = reachable_expressions(&self.storage, roots);
        let mut unsafe_instances = None;

        let mut candidates = Vec::new();
        for (key, mut occurrences) in occurrences {
            let mut seen = FxHashSet::default();
            occurrences
                .expressions
                .retain(|expression| reachable.contains(expression) && seen.insert(*expression));
            if occurrences.expressions.len() < 2 {
                continue;
            }

            // Evidence construction is shareable by compiler contract, but forcing
            // a local recursive initializer during module initialization is not.
            let unsafe_instances = unsafe_instances
                .get_or_insert_with(|| unsafe_local_instances(&self.storage, declarations));
            if key.contains_unsafe_instance(&self.evidence_keys, unsafe_instances) {
                continue;
            }

            candidates.push((key, occurrences));
        }

        candidates.sort_by_key(|(key, occurrences)| {
            let first = occurrences.expressions[0].into_raw().into_u32();
            (key.dependency_order(&self.evidence_keys), first)
        });

        for (key, occurrences) in candidates {
            let name = match occurrences.constraint_name {
                Some(name) => name,
                None => self.closed_evidence_name(&key)?,
            };
            let global = self.fresh_generated_global(name)?;
            let replacement = ExpressionKind::Global { global: Global::clone(&global) };

            let (first, remaining) = occurrences
                .expressions
                .split_first()
                .expect("invariant violated: repeated evidence has no occurrence");
            let initializer =
                self.storage.replace_expression_kind(*first, ExpressionKind::clone(&replacement));
            let initializer = self.storage.allocate_expression(Expression { kind: initializer });

            for &expression in remaining {
                self.storage
                    .replace_expression_kind(expression, ExpressionKind::clone(&replacement));
            }

            let declaration = Declaration {
                global,
                exported: false,
                recursive_group: None,
                kind: DeclarationKind::Value(initializer),
            };
            declarations.push(declaration);
        }

        Ok(())
    }

    pub(super) fn record_closed_member_selection(
        &mut self,
        function: ExpressionId,
        evidence_variable: EvidenceVarId,
        constraint: checking::TypeId,
        selection: ExpressionId,
    ) -> ConversionResult<ExpressionId> {
        let ExpressionKind::Global { global } = &self.storage[function].kind else {
            return Ok(selection);
        };
        let global = Global::clone(global);

        let GlobalId::Term(file_id, term_id) = global.id else {
            return Ok(selection);
        };
        let indexed = self.indexed_module(file_id)?;
        let indexing::IndexedTermItemKind::ClassMember { .. } = indexed.items[term_id].kind else {
            return Ok(selection);
        };

        let EvidenceState::Solved(evidence_id) = self.checked.evidence[evidence_variable].state
        else {
            return Ok(selection);
        };
        let evidence = self.evidence_key(evidence_id);
        if !self.evidence_keys.is_closed(evidence)
            || !self.evidence_keys.is_self_contained(evidence)
        {
            return Ok(selection);
        }

        let dictionary_name = self.evidence_parameter_name(constraint)?;
        let member_name = uppercase_initial(&global.item_name);
        let name = format_smolstr!("{dictionary_name}{member_name}");

        let key = ClosedEvidenceKey::Member { member: (file_id, term_id), evidence };
        self.evidence_hoisting.record(key, selection, Some(name));

        Ok(selection)
    }

    fn shared_evidence(
        &mut self,
        occurrence: &mut EvidenceOccurrence,
        evidence: EvidenceKey,
    ) -> ConversionResult<Option<ExpressionId>> {
        let parameter = if self.evidence_keys.is_self_contained(evidence) {
            if self.evidence_keys.is_closed(evidence) {
                return Ok(None);
            }
            let Some(scope) = self.evidence_scopes.last() else { return Ok(None) };
            let Some(construction) = scope.constructions.get(&evidence).cloned() else {
                return Ok(None);
            };
            match construction {
                EvidenceConstruction::Shared(parameter) => parameter,
                EvidenceConstruction::Inline { expression, name } => {
                    // The first occurrence stays inline until repetition justifies a binding.
                    // Replacing its arena node with a local updates that occurrence without a
                    // separate tree pass.
                    let parameter = self.fresh_parameter(name)?;
                    let local = ExpressionKind::Local { parameter: Parameter::clone(&parameter) };
                    let construction = self.storage.replace_expression_kind(expression, local);
                    let construction = self.expression(construction);
                    let scope = self
                        .evidence_scopes
                        .last_mut()
                        .expect("invariant violated: evidence scope disappeared during conversion");
                    scope.constructions.insert(
                        evidence,
                        EvidenceConstruction::Shared(Parameter::clone(&parameter)),
                    );
                    scope.bindings.push(EvidenceBinding {
                        evidence,
                        binding: Binding {
                            parameter: Parameter::clone(&parameter),
                            expression: construction,
                            source_order: 0,
                        },
                    });
                    parameter
                }
            }
        } else {
            let Some(construction) = occurrence.constructions.get(&evidence).cloned() else {
                return Ok(None);
            };
            match construction {
                OccurrenceConstruction::Shared(parameter) => parameter,
                OccurrenceConstruction::Inline { expression, name, order } => {
                    let parameter = self.fresh_parameter(name)?;
                    let local = ExpressionKind::Local { parameter: Parameter::clone(&parameter) };
                    let construction = self.storage.replace_expression_kind(expression, local);
                    let construction = self.expression(construction);
                    occurrence.constructions.insert(
                        evidence,
                        OccurrenceConstruction::Shared(Parameter::clone(&parameter)),
                    );
                    occurrence.bindings.push(OccurrenceBinding {
                        order,
                        binding: Binding {
                            parameter: Parameter::clone(&parameter),
                            expression: construction,
                            source_order: order,
                        },
                    });
                    parameter
                }
            }
        };
        Ok(Some(self.expression(ExpressionKind::Local { parameter })))
    }

    fn record_evidence(
        &mut self,
        occurrence: &mut EvidenceOccurrence,
        evidence: EvidenceKey,
        construction: ExpressionId,
        name: SmolStr,
        constraint: Option<checking::TypeId>,
    ) -> ConversionResult<ExpressionId> {
        if self.evidence_keys.is_self_contained(evidence) {
            if self.evidence_keys.is_closed(evidence) {
                let name = constraint
                    .map(|constraint| self.evidence_parameter_name(constraint))
                    .transpose()?;
                let key = ClosedEvidenceKey::Dictionary(evidence);
                self.evidence_hoisting.record(key, construction, name);
                return Ok(construction);
            }
            let Some(scope) = self.evidence_scopes.last_mut() else { return Ok(construction) };
            scope
                .constructions
                .insert(evidence, EvidenceConstruction::Inline { expression: construction, name });
            return Ok(construction);
        }

        let order = occurrence.next_order;
        occurrence.next_order += 1;
        if self.evidence_keys.is_cut_point(evidence) {
            let parameter = self.fresh_parameter(name)?;
            occurrence
                .constructions
                .insert(evidence, OccurrenceConstruction::Shared(Parameter::clone(&parameter)));
            occurrence.bindings.push(OccurrenceBinding {
                order,
                binding: Binding {
                    parameter: Parameter::clone(&parameter),
                    expression: construction,
                    source_order: order,
                },
            });
            // Dictionary applications are intentionally not simple expressions, so the functional
            // optimizer preserves this binding and the bounded-depth contract for later backends.
            return Ok(self.expression(ExpressionKind::Local { parameter }));
        }
        occurrence.constructions.insert(
            evidence,
            OccurrenceConstruction::Inline { expression: construction, name, order },
        );
        Ok(construction)
    }

    fn evidence_key(&mut self, evidence: EvidenceId) -> EvidenceKey {
        self.evidence_keys.key(&self.checked.evidence, evidence)
    }

    fn evidence_dictionary_name(&self, evidence: EvidenceKey) -> ConversionResult<SmolStr> {
        let base = self.evidence_name_base(evidence)?;
        Ok(format_smolstr!("{base}Dict"))
    }

    fn closed_evidence_name(&self, evidence: &ClosedEvidenceKey) -> ConversionResult<SmolStr> {
        match evidence {
            ClosedEvidenceKey::Dictionary(evidence) => self.evidence_dictionary_name(*evidence),
            ClosedEvidenceKey::Member { member: (file_id, term_id), evidence } => {
                let dictionary_name = self.evidence_dictionary_name(*evidence)?;
                let indexed = self.indexed_module(*file_id)?;
                let member_name = indexed.items[*term_id]
                    .name
                    .as_ref()
                    .map_or_else(|| String::from("Member"), |name| uppercase_initial(name));
                Ok(format_smolstr!("{dictionary_name}{member_name}"))
            }
        }
    }

    fn evidence_name_base(&self, evidence: EvidenceKey) -> ConversionResult<SmolStr> {
        let mut name = match self.evidence_keys.kind(evidence) {
            EvidenceKeyKind::Instance { origin, .. } => {
                let identity = instance_identity(*origin);
                self.instance_name(identity)?.to_string()
            }
            EvidenceKeyKind::Superclass { parent, superclass } => {
                let parent = self.evidence_name_base(*parent)?;
                let field = self.superclass_field(*superclass)?;
                format!("{parent}{}", uppercase_initial(&field.name))
            }
            EvidenceKeyKind::Given(_)
            | EvidenceKeyKind::Opaque(_)
            | EvidenceKeyKind::InvalidEvidence(_)
            | EvidenceKeyKind::InvalidVariable(_) => String::from("evidence"),
        };

        if let EvidenceKeyKind::Instance { subgoals, .. } = self.evidence_keys.kind(evidence) {
            for subgoal in subgoals {
                let subgoal = self.evidence_name_base(*subgoal)?;
                name.push_str(&uppercase_initial(&subgoal));
            }
        }

        Ok(SmolStr::new(name))
    }

    pub(super) fn evidence_parameter(
        &mut self,
        binder: EvidenceBinderId,
    ) -> ConversionResult<Parameter> {
        let constraint = self.checked.evidence[binder].constraint;
        let name = self.evidence_parameter_name(constraint)?;
        self.parameter(BindingSource::Evidence(binder), name)
    }

    fn evidence_parameter_name(&self, constraint: checking::TypeId) -> QueryResult<SmolStr> {
        let mut current = constraint;
        let mut arguments = vec![];
        loop {
            match *self.queries.lookup_type(current) {
                checking::Type::Application(function, argument) => {
                    arguments.push(argument);
                    current = function;
                }
                checking::Type::KindApplication(function, _)
                | checking::Type::Kinded(function, _) => current = function,
                checking::Type::Constructor(file_id, type_id) => {
                    let Some(class_name) = self.type_item_name(file_id, type_id)? else {
                        return Ok(SmolStr::new("dictionary"));
                    };
                    let Some(mut name) = lowercase_initial(&class_name) else {
                        return Ok(SmolStr::new("dictionary"));
                    };
                    let mut fragments = 0;
                    for argument in arguments.into_iter().rev() {
                        self.append_evidence_type_name(&mut name, argument, &mut fragments)?;
                    }
                    name.push_str("Dict");
                    return Ok(SmolStr::new(name));
                }
                _ => return Ok(SmolStr::new("dictionary")),
            }
        }
    }

    fn append_evidence_type_name(
        &self,
        name: &mut String,
        type_id: checking::TypeId,
        fragments: &mut usize,
    ) -> QueryResult<()> {
        if *fragments >= MAX_EVIDENCE_NAME_FRAGMENTS {
            return Ok(());
        }
        match *self.queries.lookup_type(type_id) {
            checking::Type::Application(function, argument) => {
                self.append_evidence_type_name(name, function, fragments)?;
                self.append_evidence_type_name(name, argument, fragments)?;
            }
            checking::Type::KindApplication(function, _) | checking::Type::Kinded(function, _) => {
                self.append_evidence_type_name(name, function, fragments)?;
            }
            checking::Type::Forall(_, inner) | checking::Type::Constrained(_, inner) => {
                self.append_evidence_type_name(name, inner, fragments)?;
            }
            checking::Type::Function(_, _) => {
                append_evidence_name_fragment(name, "Function", fragments)
            }
            checking::Type::Constructor(file_id, type_id) => {
                if let Some(type_name) = self.type_item_name(file_id, type_id)? {
                    append_evidence_name_fragment(name, &type_name, fragments);
                }
            }
            checking::Type::Row(_) => append_evidence_name_fragment(name, "Row", fragments),
            checking::Type::Rigid(rigid, _, _) => {
                if let Some(type_name) = self.rigid_type_name(rigid)? {
                    append_evidence_name_fragment(name, &type_name, fragments);
                }
            }
            checking::Type::Integer(_)
            | checking::Type::String(..)
            | checking::Type::Unification(_)
            | checking::Type::Free(_)
            | checking::Type::Unknown(_) => {}
        }
        Ok(())
    }
}

fn append_evidence_name_fragment(name: &mut String, fragment: &str, fragments: &mut usize) {
    if *fragments >= MAX_EVIDENCE_NAME_FRAGMENTS {
        return;
    }
    let mut characters = fragment.chars();
    let Some(first) = characters.next() else { return };
    name.extend(first.to_uppercase());
    name.push_str(characters.as_str());
    *fragments += 1;
}

fn instance_identity(origin: InstanceCandidateOrigin) -> InstanceIdentity {
    match origin {
        InstanceCandidateOrigin::Instance(file_id, instance) => {
            InstanceIdentity::Declared(file_id, instance)
        }
        InstanceCandidateOrigin::Derive(file_id, derive) => {
            InstanceIdentity::Derived(file_id, derive)
        }
    }
}

fn evidence_contains_unsafe_instance(
    keys: &EvidenceKeys,
    evidence: EvidenceKey,
    unsafe_instances: &FxHashSet<InstanceIdentity>,
) -> bool {
    match keys.kind(evidence) {
        EvidenceKeyKind::Given(_)
        | EvidenceKeyKind::Opaque(_)
        | EvidenceKeyKind::InvalidEvidence(_)
        | EvidenceKeyKind::InvalidVariable(_) => false,
        EvidenceKeyKind::Instance { origin, subgoals } => {
            unsafe_instances.contains(&instance_identity(*origin))
                || subgoals.iter().any(|subgoal| {
                    evidence_contains_unsafe_instance(keys, *subgoal, unsafe_instances)
                })
        }
        EvidenceKeyKind::Superclass { parent, .. } => {
            evidence_contains_unsafe_instance(keys, *parent, unsafe_instances)
        }
    }
}

fn unsafe_local_instances(
    storage: &Storage,
    declarations: &[Declaration],
) -> FxHashSet<InstanceIdentity> {
    let has_instances = declarations.iter().any(|declaration| {
        matches!(declaration.global.id, GlobalId::Instance(_))
            && matches!(declaration.kind, DeclarationKind::Value(_))
    });
    if !has_instances {
        return FxHashSet::default();
    }

    let values = declarations.iter().filter_map(|declaration| match declaration.kind {
        DeclarationKind::Value(expression) => {
            Some((declaration.global.id, declaration.recursive_group, expression))
        }
        DeclarationKind::Constructor { .. } | DeclarationKind::Foreign => None,
    });
    let values = values.collect_vec();

    let positions = values.iter().enumerate().map(|(position, (global, _, _))| (*global, position));
    let positions = positions.collect::<FxHashMap<_, _>>();

    let mut dependencies = vec![Vec::new(); values.len()];
    for (position, (_, _, expression)) in values.iter().enumerate() {
        let globals = expression_globals(storage, *expression);
        let dependency_positions =
            globals.into_iter().filter_map(|global| positions.get(&global).copied());
        dependencies[position].extend(dependency_positions);
        dependencies[position].sort_unstable();
        dependencies[position].dedup();
    }

    let mut hazards = FxHashSet::default();
    for (position, (_, recursive_group, _)) in values.iter().enumerate() {
        let mut visited = FxHashSet::default();
        if recursive_group.is_some()
            || reaches_declaration(position, position, &dependencies, &mut visited)
        {
            hazards.insert(position);
        }
    }
    if hazards.is_empty() {
        return FxHashSet::default();
    }

    let mut unsafe_instances = FxHashSet::default();
    for (position, (global, _, _)) in values.iter().enumerate() {
        let GlobalId::Instance(identity) = global else {
            continue;
        };
        let mut pending = vec![position];
        let mut visited = FxHashSet::default();
        let mut unsafe_instance = false;
        while let Some(dependency) = pending.pop() {
            if !visited.insert(dependency) {
                continue;
            }

            if hazards.contains(&dependency) {
                unsafe_instance = true;
                break;
            }

            let next_dependencies = dependencies[dependency].iter().copied();
            pending.extend(next_dependencies);
        }

        if unsafe_instance {
            unsafe_instances.insert(*identity);
        }
    }

    unsafe_instances
}

fn reaches_declaration(
    current: usize,
    target: usize,
    dependencies: &[Vec<usize>],
    visited: &mut FxHashSet<usize>,
) -> bool {
    for &dependency in &dependencies[current] {
        if dependency == target {
            return true;
        }
        if visited.insert(dependency)
            && reaches_declaration(dependency, target, dependencies, visited)
        {
            return true;
        }
    }
    false
}

fn uppercase_initial(name: &str) -> String {
    let mut characters = name.chars();
    let Some(first) = characters.next() else { return String::new() };
    let first = first.to_uppercase().collect::<String>();
    format!("{first}{}", characters.as_str())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use checking::evidence::{Evidences, ReflectableEvidence, SynthesizedEvidence};

    use super::*;

    #[test]
    fn cut_points_account_for_branching_evidence() {
        let origin = InstanceCandidateOrigin::Instance(
            files::FileId::new(0),
            indexing::InstanceId::new(NonZeroU32::MIN),
        );
        let mut keys = EvidenceKeys::default();

        let mut deep = keys.intern(EvidenceKeyKind::Opaque(EvidenceId(0)));
        for _ in 0..MAX_INLINE_EVIDENCE_DEPTH {
            deep = keys.intern(EvidenceKeyKind::Instance { origin, subgoals: vec![deep] });
        }

        let mut sibling = keys.intern(EvidenceKeyKind::Opaque(EvidenceId(1)));
        for _ in 1..MAX_INLINE_EVIDENCE_DEPTH {
            sibling = keys.intern(EvidenceKeyKind::Instance { origin, subgoals: vec![sibling] });
        }

        let root = keys.intern(EvidenceKeyKind::Instance { origin, subgoals: vec![deep, sibling] });

        assert_eq!(keys.dependency_order(root), MAX_INLINE_EVIDENCE_DEPTH + 1);
        assert!(keys.is_cut_point(root));
    }

    #[test]
    fn invalid_evidence_preserves_structural_depth() {
        const DEPTH: usize = 10_000;

        let mut evidences = Evidences::default();
        let mut subgoal = evidences.fresh_variable();
        let trivial = evidences.allocate(Evidence::Trivial);
        evidences.solve(subgoal, trivial);

        let origin = InstanceCandidateOrigin::Instance(
            files::FileId::new(0),
            indexing::InstanceId::new(NonZeroU32::MIN),
        );
        let mut root = None;
        for _ in 0..DEPTH {
            let invalid = evidences.fresh_variable();
            evidences.mark_error(invalid);
            let evidence =
                evidences.allocate(Evidence::Instance { origin, subgoals: vec![invalid, subgoal] });
            subgoal = evidences.fresh_variable();
            evidences.solve(subgoal, evidence);
            root = Some(evidence);
        }

        let mut keys = EvidenceKeys::default();
        let root = keys.key(&evidences, root.expect("test evidence chain is empty"));

        assert_eq!(keys.dependency_order(root), DEPTH);
        assert_eq!(keys.evidence.len(), DEPTH + 1);
    }

    #[test]
    fn opaque_evidence_keys_are_closed_and_identity_tied() {
        let mut evidences = Evidences::default();
        let trivial = evidences.allocate(Evidence::Trivial);
        let synthesized = Evidence::Synthesized(SynthesizedEvidence::Reflectable(
            ReflectableEvidence::Integer(42),
        ));
        let first_synthesized_id = evidences.allocate(Evidence::clone(&synthesized));
        let second_synthesized_id = evidences.allocate(synthesized);

        let mut keys = EvidenceKeys::default();
        let trivial = keys.key(&evidences, trivial);
        let first_synthesized = keys.key(&evidences, first_synthesized_id);
        let repeated_synthesized = keys.key(&evidences, first_synthesized_id);
        let second_synthesized = keys.key(&evidences, second_synthesized_id);

        assert!(keys.is_closed(trivial));
        assert!(keys.is_closed(first_synthesized));
        assert!(first_synthesized == repeated_synthesized);
        assert!(first_synthesized != second_synthesized);
    }
}
