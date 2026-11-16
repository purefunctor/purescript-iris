pub mod context;
pub mod core;
pub mod error;
pub mod interners;
pub mod pretty;
pub mod safety;
pub mod source;
pub mod state;
pub mod tree;
pub mod types;

use std::sync::Arc;

use building_types::{QueryProxy, QueryResult};
use files::FileId;
use indexing::{DeriveItemId, InstanceChainId, InstanceItemId, TermItemId, TypeItemId};
use rustc_hash::FxHashMap;

use crate::error::CheckingError;
pub use crate::interners::Interners;
use crate::types::{ForallBinderId, Role, TypeId};

pub trait ExternalQueries:
    QueryProxy<
        Parsed = parsing::FullParsedModule,
        Stabilized = Arc<stabilizing::StabilizedModule>,
        Indexed = Arc<indexing::IndexedModule>,
        Lowered = Arc<lowering::LoweredModule>,
        Grouped = Arc<lowering::GroupedModule>,
        Resolved = Arc<resolving::ResolvedModule>,
        Bracketed = Arc<sugar::Bracketed>,
        Sectioned = Arc<sugar::Sectioned>,
    >
{
    fn checked_v2(&self, id: FileId) -> QueryResult<Arc<CheckedModule>>;

    fn interners(&self) -> &Interners;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedSynonym {
    pub kind_binders: Arc<[ForallBinderId]>,
    pub parameters: Arc<[ForallBinderId]>,
    pub body: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedData {
    pub newtype: bool,
    pub kind_binders: Arc<[ForallBinderId]>,
    pub parameters: Arc<[ForallBinderId]>,
    pub constructors: Arc<[CheckedConstructor]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedConstructor {
    pub item: TermItemId,
    pub fields: Arc<[TypeId]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionalDependency {
    pub determiners: Arc<[usize]>,
    pub determined: Arc<[usize]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedClass {
    pub kind_binders: Arc<[ForallBinderId]>,
    pub parameters: Arc<[ForallBinderId]>,
    pub superclasses: Arc<[TypeId]>,
    pub functional_dependencies: Arc<[FunctionalDependency]>,
    pub members: Arc<[(TermItemId, TypeId)]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstanceOrigin {
    Instance(InstanceItemId),
    Derive(DeriveItemId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedInstance {
    pub origin: InstanceOrigin,
    pub class: (FileId, TypeItemId),
    pub binders: Arc<[ForallBinderId]>,
    pub constraints: Arc<[TypeId]>,
    pub kind_arguments: Arc<[TypeId]>,
    pub arguments: Arc<[TypeId]>,
    pub chain: Option<(InstanceChainId, u32)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckedModule {
    pub type_kinds: FxHashMap<TypeItemId, TypeId>,
    pub term_types: FxHashMap<TermItemId, TypeId>,
    pub synonyms: FxHashMap<TypeItemId, CheckedSynonym>,
    pub data: FxHashMap<TypeItemId, CheckedData>,
    pub classes: FxHashMap<TypeItemId, CheckedClass>,
    pub instances: Vec<CheckedInstance>,
    pub roles: FxHashMap<TypeItemId, Arc<[Role]>>,
    pub tree: tree::CheckedTree,
    pub errors: Vec<CheckingError>,
}

pub fn check_module(queries: &impl ExternalQueries, file_id: FileId) -> QueryResult<CheckedModule> {
    let mut state = state::CheckState::new(file_id);
    let context = context::CheckContext::new(queries, file_id)?;

    source::type_items::check_type_items(&mut state, &context)?;
    source::instances::check_instance_heads(&mut state, &context)?;
    source::term_items::check_term_items(&mut state, &context)?;
    source::instances::check_instance_bodies(&mut state, &context)?;
    source::finish(&mut state, &context)?;

    Ok(state.checked)
}
