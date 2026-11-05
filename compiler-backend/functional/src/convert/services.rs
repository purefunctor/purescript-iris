use std::sync::Arc;

use checking::evidence::{Evidence, EvidenceBinderId, EvidenceState, EvidenceVarId};
use checking::tree as checking_tree;
use files::FileId;
use indexing::TermItemId;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};

use crate::error::UnsupportedState;
use crate::tree::{ExpressionId, ExpressionKind, Parameter, RecordField};

use super::evidence::evidence_variable;
use super::expression::convert_expression;
use super::{Context, ConversionResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ServiceSlot {
    Separate,
    Member { group: usize, key: SmolStr },
}

pub(super) type ServicePlan = Arc<[ServiceSlot]>;

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn service_plan(
        &self,
        abstractions: &[checking_tree::DeclarationAbstraction],
    ) -> ConversionResult<Option<ServicePlan>> {
        let mut slots = Vec::new();
        let mut groups = FxHashMap::<checking::TypeId, (usize, Vec<SmolStr>)>::default();
        for abstraction in abstractions {
            let constraint = match abstraction {
                checking_tree::DeclarationAbstraction::Type { .. } => continue,
                checking_tree::DeclarationAbstraction::Argument => break,
                checking_tree::DeclarationAbstraction::Evidence { constraint, .. } => *constraint,
            };
            let Some((key, row)) = self.service_constraint(constraint)? else {
                slots.push(ServiceSlot::Separate);
                continue;
            };
            let next = groups.len();
            let (group, keys) = groups.entry(row).or_insert_with(|| (next, Vec::new()));
            if keys.contains(&key) {
                slots.push(ServiceSlot::Separate);
                continue;
            }
            keys.push(SmolStr::clone(&key));
            slots.push(ServiceSlot::Member { group: *group, key });
        }
        if groups.is_empty() {
            return Ok(None);
        }
        Ok(Some(slots.into()))
    }

    fn service_constraint(
        &self,
        constraint: checking::TypeId,
    ) -> ConversionResult<Option<(SmolStr, checking::TypeId)>> {
        let mut current = self.constraint_head(constraint);
        let checking::Type::Application(function, row) = *self.queries.lookup_type(current) else {
            return Ok(None);
        };
        current = self.constraint_head(function);
        let checking::Type::Application(class, service) = *self.queries.lookup_type(current) else {
            return Ok(None);
        };
        let checking::Type::Constructor(class_file, class_id) =
            *self.queries.lookup_type(self.constraint_head(class))
        else {
            return Ok(None);
        };
        if self.source_module_name(class_file)? != "Iris.Effect"
            || self.type_item_name(class_file, class_id)?.as_deref() != Some("Has")
        {
            return Ok(None);
        }
        let checking::Type::Constructor(service_file, service_id) =
            *self.queries.lookup_type(self.constraint_head(service))
        else {
            return Ok(None);
        };
        let Some(service_name) = self.type_item_name(service_file, service_id)? else {
            return Ok(None);
        };
        let module_name = self.source_module_name(service_file)?;
        Ok(Some((format_smolstr!("{module_name}.{service_name}"), row)))
    }

    fn constraint_head(&self, mut current: checking::TypeId) -> checking::TypeId {
        loop {
            match *self.queries.lookup_type(current) {
                checking::Type::KindApplication(function, _)
                | checking::Type::Kinded(function, _)
                | checking::Type::ConstraintOrigin(function, _) => current = function,
                _ => return current,
            }
        }
    }

    pub(super) fn register_service_binders(
        &mut self,
        abstractions: &[checking_tree::DeclarationAbstraction],
    ) -> ConversionResult<()> {
        let Some(plan) = self.service_plan(abstractions)? else { return Ok(()) };
        let binders = abstractions.iter().filter_map(|abstraction| match abstraction {
            checking_tree::DeclarationAbstraction::Evidence { binder, .. } => Some(*binder),
            checking_tree::DeclarationAbstraction::Type { .. }
            | checking_tree::DeclarationAbstraction::Argument => None,
        });
        let mut parameters = FxHashMap::<usize, Parameter>::default();
        for (binder, slot) in binders.zip(plan.iter()) {
            let ServiceSlot::Member { group, key } = slot else { continue };
            let parameter = match parameters.get(group) {
                Some(parameter) => parameter.clone(),
                None => {
                    let parameter = self.fresh_parameter("services".into())?;
                    parameters.insert(*group, parameter.clone());
                    parameter
                }
            };
            self.service_binders.insert(binder, (parameter, SmolStr::clone(key)));
        }
        Ok(())
    }

    pub(super) fn service_binder(&self, binder: EvidenceBinderId) -> Option<&(Parameter, SmolStr)> {
        self.service_binders.get(&binder)
    }

    pub(super) fn merged_service_evidence(
        &mut self,
        parameter: Parameter,
        key: SmolStr,
    ) -> ExpressionId {
        let record = self.expression(ExpressionKind::Local { parameter });
        let field = self.label_field(key);
        let getter = self.expression(ExpressionKind::Project { record, field });
        let field = self.label_field("getService".into());
        let fields = Arc::from([RecordField { field, expression: getter }]);
        self.expression(ExpressionKind::Record { fields })
    }

    fn global_service_plan(
        &mut self,
        mut file_id: FileId,
        mut term_id: TermItemId,
    ) -> ConversionResult<Option<ServicePlan>> {
        if let Some((target_file, target_term)) = self.term_operator_target(file_id, term_id)? {
            (file_id, term_id) = (target_file, target_term);
        }
        if let Some(plan) = self.service_plans.get(&(file_id, term_id)) {
            return Ok(plan.clone());
        }
        let checked = if file_id == self.file_id {
            Arc::clone(&self.checked)
        } else {
            self.queries.checked(file_id)?
        };
        let plan = match checked.tree.lookup_term(term_id).map(|id| &checked.tree[id].kind) {
            Some(checking_tree::TermDeclarationKind::Value(value)) => {
                self.service_plan(&value.abstractions)?
            }
            _ => None,
        };
        self.service_plans.insert((file_id, term_id), plan.clone());
        Ok(plan)
    }

    pub(super) fn service_application(
        &mut self,
        expression: checking_tree::ExpressionId,
    ) -> ConversionResult<Option<ExpressionId>> {
        let checked = Arc::clone(&self.checked);
        let mut evidences = Vec::new();
        let mut base = expression;
        while let checking_tree::ExpressionKind::EvidenceApplication {
            function,
            evidence,
            constraint,
        } = checked.tree[base].kind
        {
            evidences.push((evidence, constraint));
            base = function;
        }
        evidences.reverse();
        let resolution = match checked.tree[base].kind {
            checking_tree::ExpressionKind::Variable { resolution }
            | checking_tree::ExpressionKind::RecordPun { resolution, .. } => resolution,
            _ => return Ok(None),
        };
        let checking_tree::VariableResolution::Source(lowering::TermVariableResolution::Reference(
            file_id,
            term_id,
        )) = resolution
        else {
            return Ok(None);
        };
        let Some(plan) = self.global_service_plan(file_id, term_id)? else { return Ok(None) };
        if plan.len() != evidences.len() {
            return Err(self.unsupported(UnsupportedState::ServiceEvidenceMismatch));
        }

        let mut function = convert_expression(self, base)?;
        let mut emitted = Vec::new();
        for (position, (slot, &(evidence, constraint))) in plan.iter().zip(&evidences).enumerate() {
            let dictionary = match slot {
                ServiceSlot::Separate => evidence_variable(self, evidence, Some(constraint))?,
                ServiceSlot::Member { group, .. } => {
                    if emitted.contains(group) {
                        continue;
                    }
                    emitted.push(*group);
                    let members = plan.iter().zip(&evidences).skip(position);
                    let members = members.filter_map(|(slot, &(evidence, _))| match slot {
                        ServiceSlot::Member { group: member_group, key }
                            if member_group == group =>
                        {
                            Some((evidence, SmolStr::clone(key)))
                        }
                        _ => None,
                    });
                    let members = members.collect::<Vec<_>>();
                    self.service_dictionary(&members)?
                }
            };
            function = self.synthetic_application(function, [dictionary])?;
        }
        Ok(Some(function))
    }

    fn service_dictionary(
        &mut self,
        members: &[(EvidenceVarId, SmolStr)],
    ) -> ConversionResult<ExpressionId> {
        let forwarded = members.iter().map(|(evidence, key)| {
            let binder = self.given_binder(*evidence)?;
            let (parameter, given_key) = self.service_binder(binder)?;
            (given_key == key).then(|| parameter.clone())
        });
        let forwarded = forwarded.collect::<Option<Vec<_>>>();
        if let Some(forwarded) = forwarded
            && let Some((first, rest)) = forwarded.split_first()
            && rest.iter().all(|parameter| parameter.id == first.id)
        {
            return Ok(self.expression(ExpressionKind::Local { parameter: first.clone() }));
        }
        let mut fields = Vec::with_capacity(members.len());
        for (evidence, key) in members {
            let dictionary = evidence_variable(self, *evidence, None)?;
            let expression = self.service_getter(dictionary)?;
            fields.push(RecordField { field: self.label_field(SmolStr::clone(key)), expression });
        }
        Ok(self.expression(ExpressionKind::Record { fields: fields.into() }))
    }

    fn given_binder(&self, variable: EvidenceVarId) -> Option<EvidenceBinderId> {
        let mut variable = variable;
        let mut visited = FxHashSet::default();
        loop {
            if !visited.insert(variable) {
                return None;
            }
            let EvidenceState::Solved(evidence) = self.checked.evidence[variable].state else {
                return None;
            };
            match self.checked.evidence[evidence] {
                Evidence::Variable(next) => variable = next,
                Evidence::Given(binder) => return Some(binder),
                _ => return None,
            }
        }
    }
}
