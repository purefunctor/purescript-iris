pub mod context;
pub mod error;
pub mod evidence;
pub mod holes;
pub mod implication;
pub mod safety;
pub mod source;
pub mod state;
pub mod tree;

pub mod core;
pub use core::pretty::PrettyQueries;
pub use core::{Type, TypeId};

pub mod interners;
pub use interners::CoreInterners;

use std::sync::Arc;

use building_types::{QueryProxy, QueryResult};
use files::FileId;
use indexing::{DeriveId, InstanceId, TermItemId, TypeItemId};
use itertools::Itertools;
use resolving::ResolvedModule;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::core::{
    CheckedClass, CheckedDataDeclaration, CheckedInstance, CheckedSynonym, ForallBinder,
    ForallBinderId, Name, Role, RowType, RowTypeId, SmolStrId,
};
use crate::error::CheckingError;
use crate::holes::{TermHole, TypeHole};

pub trait ExternalQueries:
    QueryProxy<
        Parsed = parsing::FullParsedModule,
        Stabilized = Arc<stabilizing::StabilizedModule>,
        Indexed = Arc<indexing::IndexedModule>,
        Lowered = Arc<lowering::LoweredModule>,
        Grouped = Arc<lowering::GroupedModule>,
        Resolved = Arc<ResolvedModule>,
        Exported = Arc<resolving::ExportedModule>,
        Bracketed = Arc<sugar::Bracketed>,
        Sectioned = Arc<sugar::Sectioned>,
        Checked = Arc<CheckedModule>,
    > + core::pretty::PrettyQueries
{
    fn checked_core(&self) -> QueryResult<Arc<context::CheckedCore>>
    where
        Self: Sized,
    {
        let core = context::CheckedCore::new(self)?;
        Ok(Arc::new(core))
    }

    fn intern_type(&self, t: Type) -> TypeId;

    fn lookup_type_flags(&self, id: TypeId) -> core::TypeFlags;

    fn intern_forall_binder(&self, b: ForallBinder) -> ForallBinderId;

    fn intern_row_type(&self, r: RowType) -> RowTypeId;

    fn intern_smol_str(&self, s: SmolStr) -> core::SmolStrId;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckedModule {
    pub evidence: evidence::Evidences,
    pub type_item_kinds: FxHashMap<TypeItemId, TypeId>,
    pub term_item_types: FxHashMap<TermItemId, TypeId>,
    pub data_declarations: FxHashMap<TypeItemId, CheckedDataDeclaration>,
    pub synonyms: FxHashMap<TypeItemId, CheckedSynonym>,
    pub classes: FxHashMap<TypeItemId, CheckedClass>,
    pub instances: FxHashMap<InstanceId, CheckedInstance>,
    pub derived_instances: FxHashMap<DeriveId, CheckedInstance>,
    pub roles: FxHashMap<TypeItemId, Arc<[Role]>>,
    pub node_types: CheckedNodeTypes,
    pub tree: tree::CheckedTree,
    pub holes: CheckedHoles,
    pub errors: Vec<CheckingError>,
    pub names: FxHashMap<Name, SmolStrId>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckedHoles {
    pub terms: FxHashMap<lowering::ExpressionId, TermHole>,
    pub types: FxHashMap<lowering::TypeId, TypeHole>,
}

/// Checked types and kinds keyed by stable node IDs from lowering.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckedNodeTypes {
    pub type_kinds: FxHashMap<lowering::TypeId, TypeId>,
    pub expressions: FxHashMap<lowering::ExpressionId, TypeId>,
    pub binders: FxHashMap<lowering::BinderId, TypeId>,
    pub lets: FxHashMap<lowering::LetBindingNameGroupId, TypeId>,
    pub puns: FxHashMap<lowering::RecordPunId, TypeId>,
    pub record_access_labels: FxHashMap<lowering::RecordAccessLabelId, TypeId>,
    pub record_fields: FxHashMap<lowering::RecordFieldId, TypeId>,
    pub record_updates: FxHashMap<lowering::RecordUpdateId, TypeId>,
    pub sections: FxHashMap<lowering::ExpressionId, TypeId>,
    pub forall_bindings: FxHashMap<lowering::TypeVariableBindingId, TypeId>,
    pub implicit_bindings: FxHashMap<(lowering::GraphNodeId, lowering::ImplicitBindingId), TypeId>,
    pub term_operators: FxHashMap<lowering::TermOperatorId, OperatorBranchTypes>,
    pub type_operators: FxHashMap<lowering::TypeOperatorId, OperatorBranchTypes>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorBranchTypes {
    pub left: TypeId,
    pub right: TypeId,
    pub result: TypeId,
}

impl CheckedModule {
    pub fn lookup_type_item_kind(&self, id: TypeItemId) -> Option<TypeId> {
        self.type_item_kinds.get(&id).copied()
    }

    pub fn lookup_term_item_type(&self, id: TermItemId) -> Option<TypeId> {
        self.term_item_types.get(&id).copied()
    }

    pub fn lookup_data_declaration(&self, id: TypeItemId) -> Option<CheckedDataDeclaration> {
        self.data_declarations.get(&id).cloned()
    }

    pub fn lookup_synonym(&self, id: TypeItemId) -> Option<CheckedSynonym> {
        self.synonyms.get(&id).cloned()
    }

    pub fn lookup_class(&self, id: TypeItemId) -> Option<CheckedClass> {
        self.classes.get(&id).cloned()
    }

    pub fn lookup_instance(&self, id: InstanceId) -> Option<CheckedInstance> {
        self.instances.get(&id).cloned()
    }

    pub fn lookup_derived_instance(&self, id: DeriveId) -> Option<CheckedInstance> {
        self.derived_instances.get(&id).cloned()
    }

    pub fn lookup_roles(&self, id: TypeItemId) -> Option<Arc<[Role]>> {
        self.roles.get(&id).cloned()
    }

    pub fn lookup_name(&self, name: Name) -> Option<SmolStrId> {
        self.names.get(&name).copied()
    }

    pub fn lookup_term_hole(&self, id: lowering::ExpressionId) -> Option<&TermHole> {
        self.holes.terms.get(&id)
    }

    pub fn lookup_type_hole(&self, id: lowering::TypeId) -> Option<&TypeHole> {
        self.holes.types.get(&id)
    }
}

impl CheckedNodeTypes {
    pub fn lookup_expression(&self, id: lowering::ExpressionId) -> Option<TypeId> {
        self.expressions.get(&id).copied()
    }

    pub fn lookup_type_kind(&self, id: lowering::TypeId) -> Option<TypeId> {
        self.type_kinds.get(&id).copied()
    }

    pub fn lookup_binder(&self, id: lowering::BinderId) -> Option<TypeId> {
        self.binders.get(&id).copied()
    }

    pub fn lookup_let(&self, id: lowering::LetBindingNameGroupId) -> Option<TypeId> {
        self.lets.get(&id).copied()
    }

    pub fn lookup_pun(&self, id: lowering::RecordPunId) -> Option<TypeId> {
        self.puns.get(&id).copied()
    }

    pub fn lookup_record_access_label(&self, id: lowering::RecordAccessLabelId) -> Option<TypeId> {
        self.record_access_labels.get(&id).copied()
    }

    pub fn lookup_record_field(&self, id: lowering::RecordFieldId) -> Option<TypeId> {
        self.record_fields.get(&id).copied()
    }

    pub fn lookup_record_update(&self, id: lowering::RecordUpdateId) -> Option<TypeId> {
        self.record_updates.get(&id).copied()
    }

    pub fn lookup_section(&self, id: lowering::ExpressionId) -> Option<TypeId> {
        self.sections.get(&id).copied()
    }

    pub fn lookup_forall_binding(&self, id: lowering::TypeVariableBindingId) -> Option<TypeId> {
        self.forall_bindings.get(&id).copied()
    }

    pub fn lookup_implicit_binding(
        &self,
        node: lowering::GraphNodeId,
        id: lowering::ImplicitBindingId,
    ) -> Option<TypeId> {
        self.implicit_bindings.get(&(node, id)).copied()
    }

    pub fn lookup_type_operator(
        &self,
        id: lowering::TypeOperatorId,
    ) -> Option<OperatorBranchTypes> {
        self.type_operators.get(&id).copied()
    }

    pub fn lookup_term_operator(
        &self,
        id: lowering::TermOperatorId,
    ) -> Option<OperatorBranchTypes> {
        self.term_operators.get(&id).copied()
    }
}

pub fn check_module(queries: &impl ExternalQueries, file_id: FileId) -> QueryResult<CheckedModule> {
    let prim_id = queries.prim_id();
    if file_id == prim_id { check_prim(queries, prim_id) } else { check_source(queries, file_id) }
}

fn check_source(queries: &impl ExternalQueries, file_id: FileId) -> QueryResult<CheckedModule> {
    let mut state = state::CheckState::new(file_id);
    let context = context::CheckContext::new(queries, file_id)?;

    source::check_type_items(&mut state, &context)?;
    source::check_term_items(&mut state, &context)?;
    state.with_zonk_cache(|state| {
        core::zonk::zonk_nodes(state, &context)?;
        core::zonk::zonk_tree(state, &context)?;
        core::zonk::zonk_evidence(state, &context)?;
        core::zonk::zonk_holes(state, &context)?;
        core::skolem::check(state, &context);
        core::zonk::zonk_errors(state, &context)
    })?;
    report_unexplained_unknown(&mut state, &context)?;
    state.checked.evidence.assert_finished();

    Ok(state.checked)
}

/// Reports an unknown type that replaced a failure without any error that explains it.
///
/// Unknown types replace syntax and types whose errors are reported elsewhere,
/// including in transitive imports. Without an explaining error, the failures
/// that the unknown type replaced would let an ill-typed module through.
fn report_unexplained_unknown<Q>(
    state: &mut state::CheckState,
    context: &context::CheckContext<Q>,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(error) = state.absorbed_unknown.take() else {
        return Ok(());
    };

    if state.checked.errors.iter().any(|error| !error.kind.is_warning())
        || has_source_errors(context.queries, context.id)?
    {
        return Ok(());
    }

    let mut visited = FxHashSet::default();
    let mut pending = imported_files(&context.resolved).collect_vec();
    while let Some(file_id) = pending.pop() {
        if !visited.insert(file_id) {
            continue;
        }
        if has_checking_errors(context.queries, file_id)?
            || has_source_errors(context.queries, file_id)?
        {
            return Ok(());
        }
        let resolved = context.queries.resolved(file_id)?;
        pending.extend(imported_files(&resolved));
    }

    state.checked.errors.push(error);
    Ok(())
}

pub fn has_checking_errors(queries: &impl ExternalQueries, file_id: FileId) -> QueryResult<bool> {
    let checked = queries.checked(file_id)?;
    Ok(checked.errors.iter().any(|error| !error.kind.is_warning()))
}

pub fn has_source_errors(queries: &impl ExternalQueries, file_id: FileId) -> QueryResult<bool> {
    let (_, parse_errors) = queries.parsed(file_id)?;
    let lowered = queries.lowered(file_id)?;
    let grouped = queries.grouped(file_id)?;
    let resolved = queries.resolved(file_id)?;
    Ok(!parse_errors.is_empty()
        || !lowered.errors.is_empty()
        || !grouped.cycle_errors.is_empty()
        || !resolved.errors.is_empty())
}

fn imported_files(resolved: &ResolvedModule) -> impl Iterator<Item = FileId> + '_ {
    let unqualified = resolved.unqualified.values().flatten();
    let qualified = resolved.qualified.values().flatten();
    unqualified.chain(qualified).map(|import| import.file)
}

fn check_prim(queries: &impl ExternalQueries, file_id: FileId) -> QueryResult<CheckedModule> {
    let mut checked = CheckedModule::default();
    let resolved = queries.resolved(file_id)?;

    let lookup_type = |name: &str| {
        let prim_type = resolved.exports.lookup_type(name);
        prim_type.unwrap_or_else(|| unreachable!("invariant violated: {name} not in Prim"))
    };

    let lookup_class = |name: &str| {
        let prim_class = resolved.exports.lookup_class(name);
        prim_class.unwrap_or_else(|| unreachable!("invariant violated: {name} not in Prim"))
    };

    let type_core = {
        let (file_id, item_id) = lookup_type("Type");
        queries.intern_type(Type::Constructor(file_id, item_id))
    };

    let row_core = {
        let (file_id, item_id) = lookup_type("Row");
        queries.intern_type(Type::Constructor(file_id, item_id))
    };

    let constraint_core = {
        let (file_id, item_id) = lookup_type("Constraint");
        queries.intern_type(Type::Constructor(file_id, item_id))
    };

    let type_to_type = queries.intern_type(Type::Function(type_core, type_core));
    let function_kind = queries.intern_type(Type::Function(type_core, type_to_type));

    let row_type = queries.intern_type(Type::Application(row_core, type_core));
    let record_kind = queries.intern_type(Type::Function(row_type, type_core));

    let mut insert_type = |name: &str, id: TypeId| {
        let (_, item_id) = lookup_type(name);
        checked.type_item_kinds.insert(item_id, id);
    };

    insert_type("Type", type_core);
    insert_type("Function", function_kind);
    insert_type("Array", type_to_type);
    insert_type("Record", record_kind);
    insert_type("Number", type_core);
    insert_type("Int", type_core);
    insert_type("String", type_core);
    insert_type("Char", type_core);
    insert_type("Boolean", type_core);
    insert_type("Constraint", type_core);
    insert_type("Symbol", type_core);
    insert_type("Row", type_to_type);

    let (_, partial_id) = lookup_class("Partial");
    checked.type_item_kinds.insert(partial_id, constraint_core);

    let mut insert_roles = |name: &str, roles: &[Role]| {
        let (_, item_id) = lookup_type(name);
        checked.roles.insert(item_id, Arc::from(roles));
    };

    insert_roles("Type", &[]);
    insert_roles("Function", &[Role::Representational, Role::Representational]);
    insert_roles("Array", &[Role::Representational]);
    insert_roles("Record", &[Role::Representational]);
    insert_roles("Number", &[]);
    insert_roles("Int", &[]);
    insert_roles("String", &[]);
    insert_roles("Char", &[]);
    insert_roles("Boolean", &[]);
    insert_roles("Constraint", &[]);
    insert_roles("Symbol", &[]);
    insert_roles("Row", &[Role::Representational]);

    Ok(checked)
}
