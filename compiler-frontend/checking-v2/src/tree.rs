use std::sync::Arc;

use files::FileId;
use indexing::{TermItemId, TypeItemId};
use la_arena::{Arena, Idx};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::InstanceOrigin;
use crate::types::TypeId;

pub type ExpressionId = Idx<Expression>;
pub type BinderId = Idx<Binder>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceSlotId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceBinderId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinEvidence {
    Empty,
    Symbol(SmolStr),
    Integer(i32),
    Boolean(bool),
    Ordering(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    Unsolved,
    Binder(EvidenceBinderId),
    Instance {
        file_id: FileId,
        origin: InstanceOrigin,
        type_arguments: Arc<[TypeId]>,
        arguments: Arc<[EvidenceSlotId]>,
    },
    Superclass {
        from: Arc<Evidence>,
        class: (FileId, TypeItemId),
        index: usize,
    },
    Builtin {
        class: (FileId, TypeItemId),
        value: BuiltinEvidence,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermReference {
    Global(FileId, TermItemId),
    Constructor(FileId, TermItemId),
    Binder(BinderId),
    Let(lowering::LetBindingNameGroupId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Integer(i64),
    Number(SmolStr),
    String(SmolStr),
    Char(char),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub source: Option<lowering::ExpressionId>,
    pub type_id: TypeId,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionKind {
    Reference(TermReference),
    Instantiate {
        expression: ExpressionId,
        type_arguments: Arc<[TypeId]>,
        evidence: Arc<[EvidenceSlotId]>,
    },
    Abstract {
        type_binders: Arc<[TypeId]>,
        evidence: Arc<[(TypeId, EvidenceBinderId)]>,
        expression: ExpressionId,
    },
    Application {
        function: ExpressionId,
        argument: ExpressionId,
    },
    Lambda {
        binder: BinderId,
        body: ExpressionId,
    },
    Case {
        scrutinees: Arc<[ExpressionId]>,
        branches: Arc<[CaseBranch]>,
    },
    Let {
        bindings: Arc<[LocalBinding]>,
        body: ExpressionId,
    },
    IfThenElse {
        condition: ExpressionId,
        then: ExpressionId,
        else_: ExpressionId,
    },
    Literal(Literal),
    Array(Arc<[ExpressionId]>),
    Record(Arc<[(SmolStr, ExpressionId)]>),
    RecordAccess {
        record: ExpressionId,
        label: SmolStr,
    },
    RecordUpdate {
        record: ExpressionId,
        updates: Arc<[RecordUpdate]>,
    },
    Hole(SmolStr),
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordUpdate {
    Leaf { label: SmolStr, expression: ExpressionId },
    Branch { label: SmolStr, updates: Arc<[RecordUpdate]> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseBranch {
    pub binders: Arc<[BinderId]>,
    pub guarded: Guarded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guarded {
    Unconditional(ExpressionId),
    Conditionals(Arc<[GuardedExpression]>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedExpression {
    pub guards: Arc<[Guard]>,
    pub expression: ExpressionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guard {
    Boolean(ExpressionId),
    Pattern(BinderId, ExpressionId),
    Let(Arc<[LocalBinding]>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalBinding {
    Value(LocalValue),
    Pattern { binder: BinderId, expression: ExpressionId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalValue {
    pub source: lowering::LetBindingNameGroupId,
    pub name: SmolStr,
    pub value: ValueDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binder {
    pub source: Option<lowering::BinderId>,
    pub type_id: TypeId,
    pub kind: BinderKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinderKind {
    Variable(SmolStr),
    Wildcard,
    Named(SmolStr, BinderId),
    Literal(Literal),
    Constructor { file_id: FileId, item: TermItemId, arguments: Arc<[BinderId]> },
    Array(Arc<[BinderId]>),
    Record(Arc<[(SmolStr, BinderId)]>),
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abstraction {
    pub type_binders: Arc<[TypeId]>,
    pub evidence: Arc<[(TypeId, EvidenceBinderId)]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parameter {
    Argument(TypeId),
    Abstract(Abstraction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueDeclaration {
    pub type_id: TypeId,
    pub type_binders: Arc<[TypeId]>,
    pub evidence: Arc<[(TypeId, EvidenceBinderId)]>,
    pub parameters: Arc<[Parameter]>,
    pub equations: Arc<[Equation]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equation {
    pub binders: Arc<[BinderId]>,
    pub guarded: Guarded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceMember {
    pub item: (FileId, TermItemId),
    pub value: ValueDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDeclaration {
    pub evidence: Arc<[(TypeId, EvidenceBinderId)]>,
    pub superclasses: Arc<[EvidenceSlotId]>,
    pub members: Arc<[InstanceMember]>,
    pub obligations: Arc<[(TypeId, EvidenceSlotId)]>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckedTree {
    pub expressions: Arena<Expression>,
    pub binders: Arena<Binder>,
    pub evidence: Vec<Evidence>,
    pub values: FxHashMap<TermItemId, ValueDeclaration>,
    pub instances: FxHashMap<InstanceOrigin, InstanceDeclaration>,
}

impl CheckedTree {
    pub fn allocate_expression(
        &mut self,
        source: Option<lowering::ExpressionId>,
        type_id: TypeId,
        kind: ExpressionKind,
    ) -> ExpressionId {
        self.expressions.alloc(Expression { source, type_id, kind })
    }

    pub fn allocate_binder(
        &mut self,
        source: Option<lowering::BinderId>,
        type_id: TypeId,
        kind: BinderKind,
    ) -> BinderId {
        self.binders.alloc(Binder { source, type_id, kind })
    }

    pub fn allocate_evidence_slot(&mut self) -> EvidenceSlotId {
        let index = self.evidence.len() as u32;
        self.evidence.push(Evidence::Unsolved);
        EvidenceSlotId(index)
    }

    pub fn set_evidence(&mut self, EvidenceSlotId(index): EvidenceSlotId, evidence: Evidence) {
        let slot = self.evidence.get_mut(index as usize);
        *slot.expect("invariant violated: evidence slot out of range") = evidence;
    }

    pub fn evidence(&self, EvidenceSlotId(index): EvidenceSlotId) -> &Evidence {
        let slot = self.evidence.get(index as usize);
        slot.expect("invariant violated: evidence slot out of range")
    }
}
