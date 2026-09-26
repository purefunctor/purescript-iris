//! Implements the algorithm's core state structures.

use std::mem;
use std::rc::Rc;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::CheckContext;
use crate::core::constraint::{CanonicalConstraintId, Canonicals, ConstraintInScope};
use crate::core::exhaustive::{
    ExhaustivenessReport, Pattern, PatternConstructor, PatternId, PatternInterner, PatternKind,
};
use crate::core::substitute::RigidRenaming;
use crate::core::{Depth, Name, SkolemScope, SmolStrId, Type, TypeId, constraint};
use crate::error::{CheckingError, ErrorCrumb, ErrorKind};
use crate::evidence::{EvidenceBinderId, EvidenceVarId};
use crate::implication::{GivenConstraint, Implications, Patterns, WantedConstraint};
use crate::{CheckedModule, ExternalQueries, tree};

/// Manages [`Name`] values for [`CheckState`].
pub struct Names {
    unique: u32,
    file: FileId,
}

impl Names {
    pub fn new(file: FileId) -> Names {
        Names { unique: 0, file }
    }

    pub fn fresh(&mut self) -> Name {
        let unique = self.unique;
        self.unique += 1;
        Name { file: self.file, unique, scope: None }
    }

    pub fn fresh_scoped(&mut self) -> (Name, SkolemScope) {
        let name = self.fresh();
        let scope = SkolemScope { file: name.file, unique: name.unique };
        let name = Name { scope: Some(scope), ..name };
        (name, scope)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnificationState {
    Unsolved,
    Solved(TypeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnificationEntry {
    pub depth: Depth,
    pub kind: TypeId,
    pub state: UnificationState,
}

/// Manages unification variables for [`CheckState`].
#[derive(Debug, Default)]
pub struct Unifications {
    entries: Vec<UnificationEntry>,
    unique: u32,
    frozen: bool,
}

impl Unifications {
    pub fn fresh(&mut self, depth: Depth, kind: TypeId) -> u32 {
        assert!(!self.frozen, "invariant violated: fresh unification created while frozen");
        let unique = self.unique;

        self.unique += 1;
        self.entries.push(UnificationEntry { depth, kind, state: UnificationState::Unsolved });

        unique
    }

    pub fn get(&self, index: u32) -> &UnificationEntry {
        &self.entries[index as usize]
    }

    pub fn get_mut(&mut self, index: u32) -> &mut UnificationEntry {
        &mut self.entries[index as usize]
    }

    pub fn solve(&mut self, index: u32, solution: TypeId) {
        let frozen = self.frozen;
        let state = self.get(index).state;
        assert!(
            !frozen || matches!(state, UnificationState::Solved(_)),
            "invariant violated: unification solved while frozen"
        );
        self.get_mut(index).state = UnificationState::Solved(solution);
    }

    pub fn iter(&self) -> impl Iterator<Item = &UnificationEntry> {
        self.entries.iter()
    }

    fn freeze(&mut self) {
        self.frozen = true;
    }

    fn unfreeze(&mut self) {
        self.frozen = false;
    }
}

/// Tracks type variable bindings during kind inference.
#[derive(Default)]
pub struct Bindings {
    variables: FxHashMap<SourceTypeVariableKey, SourceTypeVariable>,
    renamings: Vec<Rc<RigidRenaming>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SourceTypeVariableKey {
    Forall(lowering::TypeVariableBindingId),
    Implicit { node: lowering::GraphNodeId, id: lowering::ImplicitBindingId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceTypeVariable {
    pub(crate) name: Name,
    pub(crate) depth: Depth,
    pub(crate) kind: TypeId,
}

impl Bindings {
    fn bind(&mut self, key: SourceTypeVariableKey, name: Name, depth: Depth, kind: TypeId) {
        self.variables.insert(key, SourceTypeVariable { name, depth, kind });
    }

    pub(crate) fn bind_forall(
        &mut self,
        id: lowering::TypeVariableBindingId,
        name: Name,
        depth: Depth,
        kind: TypeId,
    ) {
        self.bind(SourceTypeVariableKey::Forall(id), name, depth, kind);
    }

    pub(crate) fn bind_implicit(
        &mut self,
        node: lowering::GraphNodeId,
        id: lowering::ImplicitBindingId,
        name: Name,
        depth: Depth,
        kind: TypeId,
    ) {
        self.bind(SourceTypeVariableKey::Implicit { node, id }, name, depth, kind);
    }

    pub(crate) fn lookup(&self, key: SourceTypeVariableKey) -> Option<SourceTypeVariable> {
        self.variables.get(&key).copied()
    }
}

/// The core state structure threaded through the algorithm.
pub struct CheckState {
    pub checked: CheckedModule,

    pub names: Names,
    pub bindings: Bindings,
    pub patterns: PatternInterner,

    zonk_cache: Option<FxHashMap<TypeId, TypeId>>,
    pub(crate) judgments: FxHashSet<tree::ExpressionId>,

    pub unifications: Unifications,
    pub implications: Implications,
    pub canonicals: Canonicals,
    pub canonical_errors: FxHashMap<CanonicalConstraintId, Vec<ErrorKind>>,

    pub defer_expansion: bool,
    pub depth: Depth,

    pub crumbs: Vec<ErrorCrumb>,
}

impl CheckState {
    pub fn new(file_id: FileId) -> CheckState {
        CheckState {
            checked: Default::default(),
            names: Names::new(file_id),
            bindings: Default::default(),
            patterns: Default::default(),
            zonk_cache: None,
            judgments: Default::default(),
            unifications: Default::default(),
            implications: Default::default(),
            canonicals: Default::default(),
            canonical_errors: Default::default(),
            defer_expansion: Default::default(),
            depth: Depth(0),
            crumbs: Default::default(),
        }
    }

    /// Enables subtree memoization while preventing new unification solutions.
    ///
    /// Existing solutions may still be path-compressed.
    pub fn with_zonk_cache<T>(&mut self, f: impl FnOnce(&mut CheckState) -> T) -> T {
        assert!(self.zonk_cache.is_none(), "invariant violated: zonk cache enabled twice");
        self.unifications.freeze();
        self.zonk_cache = Some(FxHashMap::default());

        let result = f(self);

        self.zonk_cache = None;
        self.unifications.unfreeze();
        result
    }

    pub(crate) fn lookup_zonk_cache(&self, id: TypeId) -> Option<TypeId> {
        self.zonk_cache.as_ref().and_then(|cache| cache.get(&id)).copied()
    }

    pub(crate) fn insert_zonk_cache(&mut self, id: TypeId, result: TypeId) {
        if let Some(cache) = &mut self.zonk_cache {
            cache.insert(id, result);
        }
    }

    pub fn with_depth<T>(&mut self, f: impl FnOnce(&mut CheckState) -> T) -> T {
        let depth = self.depth.increment();

        let previous = mem::replace(&mut self.depth, depth);
        let result = f(self);
        self.depth = previous;

        result
    }

    pub fn with_defer_expansion<T>(&mut self, f: impl FnOnce(&mut CheckState) -> T) -> T {
        let previous = mem::replace(&mut self.defer_expansion, true);
        let result = f(self);
        self.defer_expansion = previous;
        result
    }

    pub fn with_error_crumb<F, T>(&mut self, crumb: ErrorCrumb, f: F) -> T
    where
        F: FnOnce(&mut CheckState) -> T,
    {
        self.crumbs.push(crumb);
        let result = f(self);
        self.crumbs.pop();
        result
    }

    pub fn fresh_unification(&mut self, queries: &impl ExternalQueries, kind: TypeId) -> TypeId {
        let unification = self.unifications.fresh(self.depth, kind);
        queries.intern_type(Type::Unification(unification))
    }

    pub fn fresh_rigid(&mut self, queries: &impl ExternalQueries, kind: TypeId) -> TypeId {
        self.fresh_rigid_named(queries, kind, None)
    }

    pub fn fresh_rigid_named(
        &mut self,
        queries: &impl ExternalQueries,
        kind: TypeId,
        text: Option<SmolStrId>,
    ) -> TypeId {
        let name = self.names.fresh();
        if let Some(text) = text {
            self.checked.names.insert(name, text);
        }
        queries.intern_type(Type::Rigid(name, self.depth, kind))
    }

    pub fn fresh_scoped_rigid_named(
        &mut self,
        queries: &impl ExternalQueries,
        kind: TypeId,
        text: Option<SmolStrId>,
    ) -> (TypeId, Name, SkolemScope) {
        let (name, scope) = self.names.fresh_scoped();
        if let Some(text) = text {
            self.checked.names.insert(name, text);
        }
        let rigid = queries.intern_type(Type::Rigid(name, self.depth, kind));
        (rigid, name, scope)
    }

    pub fn insert_error(&mut self, kind: ErrorKind) {
        let crumbs = self.crumbs.iter().copied().collect();
        self.checked.errors.push(CheckingError { kind, crumbs });
    }

    pub fn push_wanted(&mut self, constraint: TypeId) -> EvidenceVarId {
        let evidence = self.checked.evidence.fresh_variable();
        self.implications.current_mut().wanted.push_back(WantedConstraint { constraint, evidence });
        evidence
    }

    pub fn push_given(&mut self, constraint: TypeId) -> EvidenceBinderId {
        let evidence = self.checked.evidence.fresh_binder(constraint);
        self.implications.current_mut().given.push(GivenConstraint { constraint, evidence });
        evidence
    }

    pub fn allocate_expression(
        &mut self,
        type_id: TypeId,
        kind: tree::ExpressionKind,
    ) -> tree::ExpressionId {
        let expression = tree::Expression { type_id, kind };
        self.checked.tree.allocate_expression(expression)
    }

    pub(crate) fn retain_expression_judgment(&mut self, expression: tree::ExpressionId) {
        self.judgments.insert(expression);
    }

    pub fn allocate_error_expression(&mut self, type_id: TypeId) -> tree::ExpressionId {
        self.allocate_expression(type_id, tree::ExpressionKind::Error)
    }

    pub fn allocate_binder(
        &mut self,
        source: lowering::BinderId,
        type_id: TypeId,
        kind: tree::BinderKind,
    ) -> tree::BinderId {
        let source = tree::BinderSource::Binder(source);
        let binder = tree::Binder { source, type_id, kind };
        self.checked.tree.allocate_binder(binder)
    }

    pub fn allocate_generated_binder(
        &mut self,
        source: lowering::DoStatementId,
        type_id: TypeId,
        kind: tree::BinderKind,
    ) -> tree::BinderId {
        let source = tree::BinderSource::DoStatement(source);
        let binder = tree::Binder { source, type_id, kind };
        self.checked.tree.allocate_binder(binder)
    }

    pub fn allocate_derived_binder(
        &mut self,
        derive: indexing::DeriveId,
        name: SmolStrId,
        type_id: TypeId,
        kind: tree::BinderKind,
    ) -> tree::BinderId {
        let source = tree::BinderSource::Generated { derive, name };
        let binder = tree::Binder { source, type_id, kind };
        self.checked.tree.allocate_binder(binder)
    }

    pub fn allocate_operator_binder(
        &mut self,
        source: lowering::TermOperatorId,
        type_id: TypeId,
        kind: tree::BinderKind,
    ) -> tree::BinderId {
        let source = tree::BinderSource::Operator(source);
        let binder = tree::Binder { source, type_id, kind };
        self.checked.tree.allocate_binder(binder)
    }

    pub fn allocate_section_binder(
        &mut self,
        source: lowering::ExpressionId,
        type_id: TypeId,
    ) -> tree::BinderId {
        let binder = self.checked.tree.allocate_section_binder(source, type_id);
        let previous = self.checked.node_types.sections.insert(source, type_id);
        assert!(previous.is_none(), "invariant violated: section type inserted twice");
        binder
    }

    pub fn allocate_error_binder(
        &mut self,
        source: lowering::BinderId,
        type_id: TypeId,
    ) -> tree::BinderId {
        self.allocate_binder(source, type_id, tree::BinderKind::Error)
    }

    pub fn with_implication<T>(&mut self, f: impl FnOnce(&mut CheckState) -> T) -> T {
        let id = self.implications.push();
        let result = f(self);
        self.implications.pop(id);
        result
    }

    pub(crate) fn lookup_source_type_variable<Q>(
        &mut self,
        context: &CheckContext<Q>,
        key: SourceTypeVariableKey,
    ) -> QueryResult<Option<SourceTypeVariable>>
    where
        Q: ExternalQueries,
    {
        let Some(mut variable) = self.bindings.lookup(key) else { return Ok(None) };

        if self.bindings.renamings.is_empty() {
            return Ok(Some(variable));
        }

        for renaming in Vec::clone(&self.bindings.renamings) {
            variable.kind = renaming.substitute(self, context, variable.kind)?;
            if let Some((name, depth)) = renaming.replacement(variable.name) {
                variable.name = name;
                variable.depth = depth;
            }
        }

        Ok(Some(variable))
    }

    pub fn with_source_type_renaming<T>(
        &mut self,
        renaming: &Rc<RigidRenaming>,
        f: impl FnOnce(&mut CheckState) -> QueryResult<T>,
    ) -> QueryResult<T> {
        self.bindings.renamings.push(Rc::clone(renaming));
        let result = f(self);
        self.bindings.renamings.pop();

        result
    }

    pub fn solve_constraints<Q>(
        &mut self,
        context: &CheckContext<Q>,
    ) -> QueryResult<Vec<ConstraintInScope>>
    where
        Q: ExternalQueries,
    {
        constraint::solve_implication(self, context)
    }

    pub fn report_exhaustiveness(&mut self, exhaustiveness: ExhaustivenessReport) {
        if let Some(patterns) = exhaustiveness.missing {
            let crumbs = self.crumbs.iter().copied().collect();
            let patterns = Patterns { patterns: Arc::from(patterns), crumbs };
            self.implications.current_mut().patterns.push(patterns);
        }

        if !exhaustiveness.redundant.is_empty() {
            let patterns = Arc::from(exhaustiveness.redundant);
            self.insert_error(ErrorKind::RedundantPatterns { patterns });
        }
    }

    pub fn allocate_pattern(&mut self, kind: PatternKind, t: TypeId) -> PatternId {
        let pattern = Pattern { kind, t };
        self.patterns.intern(pattern)
    }

    pub fn allocate_constructor(
        &mut self,
        constructor: PatternConstructor,
        t: TypeId,
    ) -> PatternId {
        let kind = PatternKind::Constructor { constructor };
        self.allocate_pattern(kind, t)
    }

    pub fn allocate_wildcard(&mut self, t: TypeId) -> PatternId {
        self.allocate_pattern(PatternKind::Wildcard, t)
    }
}
