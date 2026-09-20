//! The lowered source tree, keyed by stable IDs derived from the CST.
//!
//! Lowering attaches semantic shape and name resolution while preserving missing or
//! malformed children with [`Option`]. The later `checking::tree` arena tree contains
//! checked and elaborated nodes.
use std::sync::Arc;

use files::FileId;
use indexing::{DeriveItemId, EquationSourceId, InstanceItemId, TermItemId, TypeItemId};
use la_arena::{Arena, ArenaMap, Idx};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::literal::StringLiteral;
use crate::source::*;
use crate::{Scc, TermVariableResolution, TypeVariableResolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringKind {
    String,
    RawString,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BinderRecordItem {
    RecordField { name: Option<SmolStr>, value: Option<BinderId> },
    RecordPun { id: RecordPunId, name: Option<SmolStr> },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BinderKind {
    Typed { binder: Option<BinderId>, type_: Option<TypeId> },
    OperatorChain { head: Option<BinderId>, tail: Arc<[OperatorPair<BinderId>]> },
    Integer { value: Option<i32> },
    Number { negative: bool, value: Option<SmolStr> },
    Constructor { resolution: Option<(FileId, TermItemId)>, arguments: Arc<[BinderId]> },
    Variable { variable: Option<SmolStr> },
    Named { named: Option<SmolStr>, binder: Option<BinderId> },
    Wildcard,
    String { kind: StringKind, value: Option<StringLiteral> },
    Char { value: Option<char> },
    Boolean { boolean: bool },
    Array { array: Arc<[BinderId]> },
    Record { record: Arc<[BinderRecordItem]> },
    Parenthesized { parenthesized: Option<BinderId> },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExpressionArgument {
    Type(Option<TypeId>),
    Term(Option<ExpressionId>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordUpdate {
    Leaf { name: Option<SmolStr>, expression: Option<ExpressionId> },
    Branch { name: Option<SmolStr>, updates: Arc<[RecordUpdate]> },
}

#[derive(Debug, PartialEq, Eq)]
pub struct CaseBranch {
    pub binders: Arc<[BinderId]>,
    pub guarded_expression: Option<GuardedExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DoStatement {
    Bind { binder: Option<BinderId>, expression: Option<ExpressionId> },
    Let { statements: Arc<[LetBindingChunk]> },
    Discard { expression: Option<ExpressionId> },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExpressionRecordItem {
    RecordField { name: Option<SmolStr>, value: Option<ExpressionId> },
    RecordPun { id: RecordPunId, name: Option<SmolStr>, resolution: Option<TermVariableResolution> },
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecordAccessLabel {
    pub id: RecordAccessLabelId,
    pub name: SmolStr,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExpressionKind {
    Typed {
        expression: Option<ExpressionId>,
        type_: Option<TypeId>,
    },
    OperatorChain {
        head: Option<ExpressionId>,
        tail: Arc<[OperatorPair<ExpressionId>]>,
    },
    InfixChain {
        head: Option<ExpressionId>,
        tail: Arc<[InfixPair<ExpressionId>]>,
    },
    Negate {
        negate: Option<TermVariableResolution>,
        expression: Option<ExpressionId>,
    },
    Application {
        function: Option<ExpressionId>,
        arguments: Arc<[ExpressionArgument]>,
    },
    IfThenElse {
        if_: Option<ExpressionId>,
        then: Option<ExpressionId>,
        else_: Option<ExpressionId>,
    },
    LetIn {
        bindings: Arc<[LetBindingChunk]>,
        expression: Option<ExpressionId>,
    },
    Lambda {
        binders: Arc<[BinderId]>,
        expression: Option<ExpressionId>,
    },
    CaseOf {
        trunk: Arc<[ExpressionId]>,
        branches: Arc<[CaseBranch]>,
    },
    Do {
        bind: Option<TermVariableResolution>,
        discard: Option<TermVariableResolution>,
        statements: Arc<[DoStatementId]>,
    },
    Ado {
        map: Option<TermVariableResolution>,
        apply: Option<TermVariableResolution>,
        pure: Option<TermVariableResolution>,
        statements: Arc<[DoStatementId]>,
        expression: Option<ExpressionId>,
    },
    Constructor {
        resolution: Option<(FileId, TermItemId)>,
    },
    Variable {
        resolution: Option<TermVariableResolution>,
    },
    OperatorName {
        resolution: Option<(FileId, TermItemId)>,
    },
    Section,
    Hole,
    String {
        kind: StringKind,
        value: Option<StringLiteral>,
    },
    Char {
        value: Option<char>,
    },
    Boolean {
        boolean: bool,
    },
    Integer {
        value: Option<i32>,
    },
    Number {
        value: Option<SmolStr>,
    },
    Array {
        array: Arc<[ExpressionId]>,
    },
    Record {
        record: Arc<[ExpressionRecordItem]>,
    },
    Parenthesized {
        parenthesized: Option<ExpressionId>,
    },
    RecordAccess {
        record: Option<ExpressionId>,
        labels: Option<Arc<[RecordAccessLabel]>>,
    },
    RecordUpdate {
        record: Option<ExpressionId>,
        updates: Arc<[RecordUpdate]>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeVariableBinding {
    pub visible: bool,
    pub id: TypeVariableBindingId,
    pub name: Option<SmolStr>,
    pub kind: Option<TypeId>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FunctionalDependency {
    pub determiners: Arc<[u8]>,
    pub determined: Arc<[u8]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeRowItem {
    pub name: Option<SmolStr>,
    pub type_: Option<TypeId>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TypeKind {
    ApplicationChain { function: Option<TypeId>, arguments: Arc<[TypeId]> },
    Arrow { argument: Option<TypeId>, result: Option<TypeId> },
    Constrained { constraint: Option<TypeId>, constrained: Option<TypeId> },
    Constructor { resolution: Option<(FileId, TypeItemId)> },
    Forall { bindings: Arc<[TypeVariableBinding]>, inner: Option<TypeId> },
    Hole,
    Integer { value: Option<i32> },
    Kinded { type_: Option<TypeId>, kind: Option<TypeId> },
    Operator { resolution: Option<(FileId, TypeItemId)> },
    OperatorChain { head: Option<TypeId>, tail: Arc<[OperatorPair<TypeId>]> },
    String { kind: StringKind, value: Option<StringLiteral> },
    Variable { name: Option<SmolStr>, resolution: Option<TypeVariableResolution> },
    Wildcard,
    EffectSet { members: Arc<[TypeId]>, tail: Option<TypeId> },
    Record { items: Arc<[TypeRowItem]>, tail: Option<TypeId> },
    Row { items: Arc<[TypeRowItem]>, tail: Option<TypeId> },
    Parenthesized { parenthesized: Option<TypeId> },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Equation {
    pub source: Option<EquationSourceId>,
    pub binders: Arc<[BinderId]>,
    pub guarded: Option<GuardedExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GuardedExpression {
    Unconditional { where_expression: Option<WhereExpression> },
    Conditionals { pattern_guarded: Arc<[PatternGuarded]> },
}

#[derive(Debug, PartialEq, Eq)]
pub struct WhereExpression {
    pub expression: Option<ExpressionId>,
    pub bindings: Arc<[LetBindingChunk]>,
}

/// Group of IDs for a let-bound name
///
/// This mirrors the [`indexing::IndexedTermItemKind::Value`] pattern for top-level
/// value declarations, where the declaration group is assigned a stable ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetBindingNameGroup {
    pub name: Option<SmolStr>,
    pub signature: Option<LetBindingSignatureId>,
    pub equations: Arc<[LetBindingEquationId]>,
}

pub type LetBindingNameGroupId = Idx<LetBindingNameGroup>;

/// Core representation of the let-bound name
///
/// This is stored in [`LoweredTree`] and can be obtained from the stable
/// ID for any given let-bound name group [`LetBindingNameGroupId`].
#[derive(Debug, PartialEq, Eq)]
pub struct LetBindingName {
    pub signature: Option<TypeId>,
    pub equations: Arc<[Equation]>,
}

/// A chunk of let bindings. Pattern bindings act as boundaries between chunks.
///
/// This structure enables 2-phase type checking for mutually recursive let bindings:
/// within a `Names` chunk, all bindings can reference each other.
#[derive(Debug, PartialEq, Eq)]
pub enum LetBindingChunk {
    /// Pattern binding (acts as boundary between name binding groups)
    Pattern {
        source: LetBindingId,
        binder: Option<BinderId>,
        where_expression: Option<WhereExpression>,
    },
    /// Group of name bindings with SCC ordering for type checking
    Names { bindings: Arc<[LetBindingNameGroupId]>, scc: Vec<Scc<LetBindingNameGroupId>> },
}

#[derive(Debug, PartialEq, Eq)]
pub struct PatternGuarded {
    pub pattern_guards: Arc<[PatternGuard]>,
    pub where_expression: Option<WhereExpression>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PatternGuard {
    pub binder: Option<BinderId>,
    pub expression: Option<ExpressionId>,
}

pub trait IsElement: Copy {
    type OperatorId: Copy;
}

impl IsElement for BinderId {
    type OperatorId = TermOperatorId;
}

impl IsElement for ExpressionId {
    type OperatorId = TermOperatorId;
}

impl IsElement for TypeId {
    type OperatorId = TypeOperatorId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperatorPair<I: IsElement> {
    pub id: Option<I::OperatorId>,
    pub element: Option<I>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InfixPair<T> {
    pub tick: Option<T>,
    pub element: Option<T>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InstanceMemberGroup {
    pub statements: Arc<[InstanceMemberId]>,
    pub resolution: Option<(FileId, TermItemId)>,
    pub signature: Option<TypeId>,
    pub equations: Arc<[Equation]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Associativity {
    /// infix
    None,
    /// infixl
    Left,
    /// infixr
    Right,
}

/// The lowered semantic contents associated with an indexed term item.
#[derive(Debug, PartialEq, Eq)]
pub enum TermItemKind {
    ClassMember {
        signature: Option<TypeId>,
    },
    Constructor {
        arguments: Arc<[TypeId]>,
    },
    Foreign {
        signature: Option<TypeId>,
    },
    Operator {
        associativity: Option<Associativity>,
        precedence: Option<u8>,
        resolution: Option<(FileId, TermItemId)>,
    },
    Value {
        signature: Option<TypeId>,
        equations: Arc<[Equation]>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct InstanceItem {
    pub constraints: Arc<[TypeId]>,
    pub resolution: Option<(FileId, TypeItemId)>,
    pub arguments: Arc<[TypeId]>,
    pub members: Arc<[InstanceMemberGroup]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeriveItem {
    pub newtype: bool,
    pub constraints: Arc<[TypeId]>,
    pub resolution: Option<(FileId, TypeItemId)>,
    pub arguments: Arc<[TypeId]>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Role {
    Nominal,
    Representational,
    Phantom,
    Unknown,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DataDeclaration {
    pub variables: Arc<[TypeVariableBinding]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NewtypeDeclaration {
    pub variables: Arc<[TypeVariableBinding]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeSynonymDeclaration {
    pub variables: Arc<[TypeVariableBinding]>,
    pub type_: Option<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub constraints: Arc<[TypeId]>,
    pub variables: Arc<[TypeVariableBinding]>,
    pub functional_dependencies: Arc<[FunctionalDependency]>,
}

/// The lowered semantic contents associated with an indexed type item.
#[derive(Debug, PartialEq, Eq)]
pub enum TypeItemKind {
    Data {
        signature: Option<TypeId>,
        declaration: Option<DataDeclaration>,
        roles: Arc<[Role]>,
    },
    Newtype {
        signature: Option<TypeId>,
        declaration: Option<NewtypeDeclaration>,
        roles: Arc<[Role]>,
    },
    Synonym {
        signature: Option<TypeId>,
        declaration: Option<TypeSynonymDeclaration>,
    },
    Class {
        signature: Option<TypeId>,
        declaration: Option<ClassDeclaration>,
    },
    Foreign {
        signature: Option<TypeId>,
        roles: Arc<[Role]>,
    },
    Operator {
        associativity: Option<Associativity>,
        precedence: Option<u8>,
        resolution: Option<(FileId, TypeItemId)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Term,
    Type,
}

/// Semantic structure and resolutions attached to stable source identities.
///
/// Unlike the arena-owned nodes in `checking::tree`, these maps retain the IDs
/// established from concrete syntax so editor queries can relate semantics back
/// to source nodes directly.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoweredTree {
    pub(crate) binders: FxHashMap<BinderId, BinderKind>,
    pub(crate) expressions: FxHashMap<ExpressionId, ExpressionKind>,
    pub(crate) types: FxHashMap<TypeId, TypeKind>,
    pub(crate) term_items: ArenaMap<TermItemId, TermItemKind>,
    pub(crate) type_items: ArenaMap<TypeItemId, TypeItemKind>,
    pub(crate) instance_items: ArenaMap<InstanceItemId, InstanceItem>,
    pub(crate) derive_items: ArenaMap<DeriveItemId, DeriveItem>,

    pub(crate) do_statements: FxHashMap<DoStatementId, DoStatement>,
    pub(crate) let_binding_groups: Arena<LetBindingNameGroup>,
    pub(crate) let_binding_names: ArenaMap<LetBindingNameGroupId, LetBindingName>,

    pub(crate) term_operators: FxHashMap<TermOperatorId, (FileId, TermItemId)>,
    pub(crate) type_operators: FxHashMap<TypeOperatorId, (FileId, TypeItemId)>,
    pub(crate) expression_puns: FxHashMap<RecordPunId, TermVariableResolution>,
}

impl LoweredTree {
    pub fn iter_binder(&self) -> impl Iterator<Item = (BinderId, &BinderKind)> {
        self.binders.iter().map(|(k, v)| (*k, v))
    }

    pub fn iter_expression(&self) -> impl Iterator<Item = (ExpressionId, &ExpressionKind)> {
        self.expressions.iter().map(|(k, v)| (*k, v))
    }

    pub fn iter_type(&self) -> impl Iterator<Item = (TypeId, &TypeKind)> {
        self.types.iter().map(|(k, v)| (*k, v))
    }

    pub fn iter_do_statement(&self) -> impl Iterator<Item = (DoStatementId, &DoStatement)> {
        self.do_statements.iter().map(|(k, v)| (*k, v))
    }

    pub fn iter_term_operator(&self) -> impl Iterator<Item = (TermOperatorId, FileId, TermItemId)> {
        self.term_operators.iter().map(|(o_id, (f_id, t_id))| (*o_id, *f_id, *t_id))
    }

    pub fn iter_type_operator(&self) -> impl Iterator<Item = (TypeOperatorId, FileId, TypeItemId)> {
        self.type_operators.iter().map(|(o_id, (f_id, t_id))| (*o_id, *f_id, *t_id))
    }

    pub fn iter_expression_pun(
        &self,
    ) -> impl Iterator<Item = (RecordPunId, TermVariableResolution)> {
        self.expression_puns.iter().map(|(k, v)| (*k, *v))
    }

    pub fn get_binder_kind(&self, id: BinderId) -> Option<&BinderKind> {
        self.binders.get(&id)
    }

    pub fn get_expression_kind(&self, id: ExpressionId) -> Option<&ExpressionKind> {
        self.expressions.get(&id)
    }

    pub fn get_type_kind(&self, id: TypeId) -> Option<&TypeKind> {
        self.types.get(&id)
    }

    pub fn get_do_statement(&self, id: DoStatementId) -> Option<&DoStatement> {
        self.do_statements.get(&id)
    }

    pub fn get_term_item_kind(&self, id: TermItemId) -> Option<&TermItemKind> {
        self.term_items.get(id)
    }

    pub fn get_type_item_kind(&self, id: TypeItemId) -> Option<&TypeItemKind> {
        self.type_items.get(id)
    }

    pub fn get_instance_item(&self, id: InstanceItemId) -> Option<&InstanceItem> {
        self.instance_items.get(id)
    }

    pub fn get_derive_item(&self, id: DeriveItemId) -> Option<&DeriveItem> {
        self.derive_items.get(id)
    }

    pub fn get_let_binding_group(&self, id: LetBindingNameGroupId) -> &LetBindingNameGroup {
        &self.let_binding_groups[id]
    }

    pub fn let_binding_group_for_signature(
        &self,
        signature: crate::source::LetBindingSignatureId,
    ) -> Option<LetBindingNameGroupId> {
        self.let_binding_groups
            .iter()
            .find_map(|(id, group)| (group.signature == Some(signature)).then_some(id))
    }

    pub fn let_binding_group_for_equation(
        &self,
        equation: crate::source::LetBindingEquationId,
    ) -> Option<LetBindingNameGroupId> {
        self.let_binding_groups
            .iter()
            .find_map(|(id, group)| group.equations.contains(&equation).then_some(id))
    }

    pub fn get_let_binding(&self, id: LetBindingNameGroupId) -> Option<&LetBindingName> {
        self.let_binding_names.get(id)
    }

    pub fn get_term_operator(&self, id: TermOperatorId) -> Option<(FileId, TermItemId)> {
        self.term_operators.get(&id).copied()
    }

    pub fn get_type_operator(&self, id: TypeOperatorId) -> Option<(FileId, TypeItemId)> {
        self.type_operators.get(&id).copied()
    }

    pub fn get_expression_pun(&self, id: RecordPunId) -> Option<TermVariableResolution> {
        self.expression_puns.get(&id).copied()
    }

    pub fn find_let_binding_group_by_signature(
        &self,
        signature_id: LetBindingSignatureId,
    ) -> Option<LetBindingNameGroupId> {
        self.let_binding_groups.iter().find_map(|(let_binding_id, let_binding_group)| {
            let_binding_group
                .signature
                .is_some_and(|candidate_id| candidate_id == signature_id)
                .then_some(let_binding_id)
        })
    }

    pub fn find_let_binding_group_by_equation(
        &self,
        equation_id: LetBindingEquationId,
    ) -> Option<LetBindingNameGroupId> {
        self.let_binding_groups.iter().find_map(|(let_binding_id, let_binding_group)| {
            let_binding_group.equations.contains(&equation_id).then_some(let_binding_id)
        })
    }

    pub fn find_instance_member_resolution(
        &self,
        statement_id: InstanceMemberId,
    ) -> Option<(FileId, TermItemId)> {
        self.instance_items.values().find_map(|item| {
            let members = &item.members;
            let member = members.iter().find(|member| member.statements.contains(&statement_id))?;
            member.resolution
        })
    }
}
