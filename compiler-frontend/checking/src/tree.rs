//! The checked semantic tree produced by type checking and elaboration.
//!
//! Nodes in this tree have complete checked structure and arena-local identities.
//! Their source provenance refers back to stable identities in `lowering::tree`.

pub mod pretty;

use std::ops::Index;
use std::sync::Arc;

use files::FileId;
use indexing::{DeriveId, DeriveItemId, EquationSourceId, InstanceItemId, TermItemId, TypeItemId};
use la_arena::{Arena, ArenaMap, Idx};
use lowering::LetBindingNameGroupId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::TypeId;
use crate::core::{ForallBinderId, Role, SmolStrId};
use crate::evidence::{Evidence, EvidenceBinderId, EvidenceVarId, SuperclassId};

pub type ExpressionId = Idx<Expression>;
pub type BinderId = Idx<Binder>;
pub type TermDeclarationId = Idx<TermDeclaration>;
pub type TypeDeclarationId = Idx<TypeDeclaration>;
pub type LocalDeclarationId = Idx<LocalDeclaration>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckedTree {
    pub(crate) arena: CheckedTreeArena,
    terms: ArenaMap<TermItemId, TermDeclarationId>,
    instances: ArenaMap<InstanceItemId, TermDeclarationId>,
    derives: ArenaMap<DeriveItemId, TermDeclarationId>,
    types: ArenaMap<TypeItemId, TypeDeclarationId>,
    lets: ArenaMap<LetBindingNameGroupId, LocalDeclarationId>,
    sections: FxHashMap<lowering::ExpressionId, BinderId>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckedTreeArena {
    pub(crate) expressions: Arena<Expression>,
    pub(crate) binders: Arena<Binder>,
    pub(crate) terms: Arena<TermDeclaration>,
    pub(crate) types: Arena<TypeDeclaration>,
    pub(crate) lets: Arena<LocalDeclaration>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TermDeclaration {
    pub type_id: TypeId,
    pub kind: TermDeclarationKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TermDeclarationKind {
    Value(ValueDeclaration),
    Foreign,
    Constructor(DataConstructor),
    Instance(InstanceDeclaration),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValueDeclaration {
    pub abstractions: Arc<[DeclarationAbstraction]>,
    pub equations: Arc<[Equation]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationAbstraction {
    Type {
        binder: ForallBinderId,
        rigid: TypeId,
    },
    Evidence {
        constraint: TypeId,
        binder: EvidenceBinderId,
    },
    /// Consume the next explicit parameter at this point in the type spine.
    Argument,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LocalDeclaration {
    pub source: LetBindingNameGroupId,
    pub type_id: TypeId,
    pub value: ValueDeclaration,
}

impl LocalDeclaration {
    pub fn new(
        source: LetBindingNameGroupId,
        type_id: TypeId,
        abstractions: Arc<[DeclarationAbstraction]>,
        equations: Arc<[Equation]>,
    ) -> LocalDeclaration {
        let value = ValueDeclaration { abstractions, equations };
        LocalDeclaration { source, type_id, value }
    }

    pub fn nullary(
        source: LetBindingNameGroupId,
        type_id: TypeId,
        equation_source: lowering::LetBindingEquationId,
        guarded_expression: GuardedExpression,
    ) -> LocalDeclaration {
        let equation = Equation::local(equation_source, [].into(), guarded_expression);
        LocalDeclaration::new(source, type_id, [].into(), [equation].into())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DataConstructor {
    pub arguments: Arc<[TypeId]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InstanceDeclaration {
    pub class: (FileId, TypeItemId),
    pub rigid_parameters: Arc<[TypeId]>,
    pub evidences: Arc<[InstanceEvidence]>,
    pub superclasses: Arc<[InstanceSuperclass]>,
    pub implementation: InstanceImplementation,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstanceImplementation {
    Members(Arc<[InstanceMember]>),
    Delegate { constraint: TypeId, evidence: EvidenceVarId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceEvidence {
    pub constraint: TypeId,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceSuperclass {
    pub id: SuperclassId,
    pub constraint: TypeId,
    pub evidence: EvidenceVarId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceMember {
    pub resolution: (FileId, TermItemId),
    pub implementation_type: TypeId,
    pub abstractions: Arc<[DeclarationAbstraction]>,
    pub equations: Arc<[Equation]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeDeclaration {
    pub kind: TypeId,
    pub roles: Arc<[Role]>,
    pub declaration: TypeDeclarationKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TypeDeclarationKind {
    Data(DataDeclaration),
    Newtype(DataDeclaration),
    Class(ClassDeclaration),
}

#[derive(Debug, PartialEq, Eq)]
pub struct DataDeclaration {
    pub parameters: Arc<[ForallBinderId]>,
    pub constructors: Arc<[TermDeclarationId]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub kind_binders: Arc<[ForallBinderId]>,
    pub type_parameters: Arc<[ForallBinderId]>,
    pub superclasses: Arc<[ClassSuperclass]>,
    pub members: Arc<[ClassMember]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassSuperclass {
    pub id: SuperclassId,
    pub constraint: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassMember {
    pub source: TermItemId,
    pub field_type: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquationSource {
    Item(EquationSourceId),
    Local(lowering::LetBindingEquationId),
    Generated { derive: DeriveId, member: TermItemId },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Equation {
    pub source: EquationSource,
    pub binders: Arc<[BinderId]>,
    pub guarded_expression: GuardedExpression,
}

impl Equation {
    pub fn item(
        source: EquationSourceId,
        binders: Arc<[BinderId]>,
        guarded_expression: GuardedExpression,
    ) -> Equation {
        let source = EquationSource::Item(source);
        Equation { source, binders, guarded_expression }
    }

    pub fn local(
        source: lowering::LetBindingEquationId,
        binders: Arc<[BinderId]>,
        guarded_expression: GuardedExpression,
    ) -> Equation {
        let source = EquationSource::Local(source);
        Equation { source, binders, guarded_expression }
    }

    pub fn generated(
        derive: DeriveId,
        member: TermItemId,
        binders: Arc<[BinderId]>,
        guarded_expression: GuardedExpression,
    ) -> Equation {
        let source = EquationSource::Generated { derive, member };
        Equation { source, binders, guarded_expression }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct GuardedExpression {
    pub alternatives: Arc<[GuardedAlternative]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GuardedAlternative {
    pub pattern_guards: Arc<[PatternGuard]>,
    pub where_expression: WhereExpression,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CaseAlternative {
    pub binders: Arc<[BinderId]>,
    pub guarded_expression: GuardedExpression,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PatternGuard {
    Boolean { expression: ExpressionId },
    Pattern { binder: BinderId, expression: ExpressionId },
}

#[derive(Debug, PartialEq, Eq)]
pub struct WhereExpression {
    pub bindings: LetBindings,
    pub expression: ExpressionId,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LetBindings {
    pub chunks: Arc<[LetBindingChunk]>,
}

impl WhereExpression {
    pub fn new(expression: ExpressionId) -> WhereExpression {
        WhereExpression { bindings: LetBindings::default(), expression }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LetBindingChunk {
    Pattern {
        source: lowering::LetBindingId,
        binder: BinderId,
        where_expression: WhereExpression,
    },
    PatternError {
        source: lowering::LetBindingId,
        binder_source: Option<lowering::BinderId>,
        where_expression: Option<WhereExpression>,
    },
    Names {
        declarations: Arc<[LocalDeclarationId]>,
        groups: Arc<[lowering::Scc<LetBindingNameGroupId>]>,
    },
}

impl GuardedExpression {
    pub fn unconditional(where_expression: WhereExpression) -> GuardedExpression {
        let alternative = GuardedAlternative { pattern_guards: Arc::from([]), where_expression };
        GuardedExpression { alternatives: Arc::from([alternative]) }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Binder {
    pub source: BinderSource,
    pub type_id: TypeId,
    pub kind: BinderKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinderSource {
    Binder(lowering::BinderId),
    DoStatement(lowering::DoStatementId),
    Operator(lowering::TermOperatorId),
    Section(lowering::ExpressionId),
    Generated { derive: DeriveId, name: SmolStrId },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BinderKind {
    Error,
    Typed { binder: BinderId, annotation: TypeId },
    Integer { value: i32 },
    Number { negative: bool, value: SmolStr },
    Variable,
    Named { name: SmolStr, binder: BinderId },
    Wildcard,
    String { value: lowering::StringLiteral },
    Char { value: char },
    Boolean { value: bool },
    Array { elements: Arc<[BinderId]> },
    Record { fields: Arc<[RecordBinderField]> },
    Constructor { resolution: (FileId, TermItemId), arguments: Arc<[BinderId]> },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordBinderField {
    Field { label: SmolStr, binder: BinderId },
    Pun { label: SmolStr },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Expression {
    pub type_id: TypeId,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableResolution {
    Source(lowering::TermVariableResolution),
    Generated(BinderId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExpressionKind {
    Error,
    String { kind: lowering::StringKind, value: lowering::StringLiteral },
    Char { value: char },
    Boolean { value: bool },
    Integer { value: i32 },
    Number { value: SmolStr },
    Array { elements: Arc<[ExpressionId]> },
    Record { fields: Arc<[RecordExpressionField]> },
    RecordAccess { record: ExpressionId, labels: Arc<[SmolStr]> },
    RecordUpdate { record: ExpressionId, updates: Arc<[RecordExpressionUpdate]> },
    Constructor { resolution: (FileId, TermItemId) },
    Variable { resolution: VariableResolution },
    RecordPun { source: lowering::RecordPunId, resolution: VariableResolution },
    Section { binder: BinderId },
    TermApplication { function: ExpressionId, argument: ExpressionId },
    EvidenceApplication { function: ExpressionId, evidence: EvidenceVarId, constraint: TypeId },
    EvidenceAbstraction { binder: EvidenceBinderId, expression: ExpressionId },
    Lambda { binders: Arc<[BinderId]>, expression: ExpressionId },
    IfThenElse { condition: ExpressionId, then: ExpressionId, else_: ExpressionId },
    Case { scrutinees: Arc<[ExpressionId]>, alternatives: Arc<[CaseAlternative]> },
    Let { bindings: LetBindings, expression: ExpressionId },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordExpressionField {
    Field { label: SmolStr, expression: ExpressionId },
    Pun { source: lowering::RecordPunId, label: SmolStr, expression: ExpressionId },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordExpressionUpdate {
    Error,
    Leaf { label: SmolStr, expression: ExpressionId },
    Branch { label: SmolStr, updates: Arc<[RecordExpressionUpdate]> },
}

impl CheckedTree {
    pub fn allocate_expression(&mut self, expression: Expression) -> ExpressionId {
        self.arena.expressions.alloc(expression)
    }

    pub(crate) fn set_expression_type(&mut self, expression: ExpressionId, type_id: TypeId) {
        self.arena.expressions[expression].type_id = type_id;
    }

    pub fn allocate_binder(&mut self, binder: Binder) -> BinderId {
        self.arena.binders.alloc(binder)
    }

    pub fn allocate_section_binder(
        &mut self,
        source: lowering::ExpressionId,
        type_id: TypeId,
    ) -> BinderId {
        let binder =
            Binder { source: BinderSource::Section(source), type_id, kind: BinderKind::Variable };
        let binder = self.arena.binders.alloc(binder);
        let previous = self.sections.insert(source, binder);
        assert!(previous.is_none(), "invariant violated: section binder allocated twice");
        binder
    }

    pub fn lookup_section_binder(&self, source: lowering::ExpressionId) -> Option<BinderId> {
        self.sections.get(&source).copied()
    }

    pub fn insert_term(&mut self, source: TermItemId, term: TermDeclaration) -> TermDeclarationId {
        let term = self.arena.terms.alloc(term);
        self.terms.insert(source, term);
        term
    }

    pub fn lookup_term(&self, source: TermItemId) -> Option<TermDeclarationId> {
        self.terms.get(source).copied()
    }

    pub fn insert_instance(
        &mut self,
        source: InstanceItemId,
        declaration: TermDeclaration,
    ) -> TermDeclarationId {
        let declaration = self.arena.terms.alloc(declaration);
        self.instances.insert(source, declaration);
        declaration
    }

    pub fn lookup_instance(&self, source: InstanceItemId) -> Option<TermDeclarationId> {
        self.instances.get(source).copied()
    }

    pub fn insert_derive(
        &mut self,
        source: DeriveItemId,
        declaration: TermDeclaration,
    ) -> TermDeclarationId {
        let declaration = self.arena.terms.alloc(declaration);
        self.derives.insert(source, declaration);
        declaration
    }

    pub fn lookup_derive(&self, source: DeriveItemId) -> Option<TermDeclarationId> {
        self.derives.get(source).copied()
    }

    pub(crate) fn iter_terms(&self) -> impl Iterator<Item = (TermItemId, TermDeclarationId)> + '_ {
        self.terms.iter().map(|(source, declaration)| (source, *declaration))
    }

    pub(crate) fn iter_instances(
        &self,
    ) -> impl Iterator<Item = (InstanceItemId, TermDeclarationId)> + '_ {
        self.instances.iter().map(|(source, declaration)| (source, *declaration))
    }

    pub(crate) fn iter_derives(
        &self,
    ) -> impl Iterator<Item = (DeriveItemId, TermDeclarationId)> + '_ {
        self.derives.iter().map(|(source, declaration)| (source, *declaration))
    }

    pub fn insert_let(&mut self, declaration: LocalDeclaration) -> LocalDeclarationId {
        let source = declaration.source;
        let declaration = self.arena.lets.alloc(declaration);
        let previous = self.lets.insert(source, declaration);
        assert!(previous.is_none(), "invariant violated: local declaration inserted twice");
        declaration
    }

    pub fn lookup_let(&self, source: LetBindingNameGroupId) -> Option<LocalDeclarationId> {
        self.lets.get(source).copied()
    }

    pub(crate) fn set_let_type(&mut self, declaration: LocalDeclarationId, type_id: TypeId) {
        self.arena.lets[declaration].type_id = type_id;
    }

    pub fn insert_type_declaration(
        &mut self,
        source: TypeItemId,
        declaration: TypeDeclaration,
    ) -> TypeDeclarationId {
        let declaration = self.arena.types.alloc(declaration);
        self.types.insert(source, declaration);
        declaration
    }

    pub fn lookup_type_declaration(&self, source: TypeItemId) -> Option<TypeDeclarationId> {
        self.types.get(source).copied()
    }
}

impl Index<ExpressionId> for CheckedTree {
    type Output = Expression;

    fn index(&self, index: ExpressionId) -> &Expression {
        &self.arena.expressions[index]
    }
}

impl Index<BinderId> for CheckedTree {
    type Output = Binder;

    fn index(&self, index: BinderId) -> &Binder {
        &self.arena.binders[index]
    }
}

impl Index<TermDeclarationId> for CheckedTree {
    type Output = TermDeclaration;

    fn index(&self, index: TermDeclarationId) -> &TermDeclaration {
        &self.arena.terms[index]
    }
}

impl Index<TypeDeclarationId> for CheckedTree {
    type Output = TypeDeclaration;

    fn index(&self, index: TypeDeclarationId) -> &TypeDeclaration {
        &self.arena.types[index]
    }
}

impl Index<LocalDeclarationId> for CheckedTree {
    type Output = LocalDeclaration;

    fn index(&self, index: LocalDeclarationId) -> &LocalDeclaration {
        &self.arena.lets[index]
    }
}
