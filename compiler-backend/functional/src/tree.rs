//! Arena-allocated functional trees produced from checked modules.

use std::ops::Index;
use std::sync::Arc;

use files::FileId;
use indexing::{DeriveId, InstanceId, TermItemId, TypeItemId};
use la_arena::{Arena, Idx};
use lowering::TypeId as SourceTypeId;
use smol_str::SmolStr;

use crate::native::NativeOperation;
use crate::stylex::StyleXExpression;

pub type ExpressionId = Idx<Expression>;
pub type PatternId = Idx<Pattern>;

#[derive(Debug, PartialEq, Eq)]
pub struct Module {
    pub file_id: FileId,
    pub name: SmolStr,
    pub dependencies: Arc<[ModuleDependency]>,
    pub surface: ModuleSurface,
    pub declarations: Arc<[Declaration]>,
    pub storage: Storage,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ModuleSurface {
    pub indirect: Arc<[IndirectModuleExports]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IndirectModuleExports {
    pub file_id: FileId,
    pub globals: Arc<[Global]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ModuleDependency {
    pub file_id: FileId,
    pub module_name: SmolStr,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Storage {
    expressions: Arena<Expression>,
    patterns: Arena<Pattern>,
}

impl Storage {
    pub fn allocate_expression(&mut self, expression: Expression) -> ExpressionId {
        self.expressions.alloc(expression)
    }

    pub fn allocate_pattern(&mut self, pattern: Pattern) -> PatternId {
        self.patterns.alloc(pattern)
    }

    pub fn expressions(&self) -> impl Iterator<Item = (ExpressionId, &Expression)> {
        self.expressions.iter()
    }

    pub fn patterns(&self) -> impl Iterator<Item = (PatternId, &Pattern)> {
        self.patterns.iter()
    }

    pub(crate) fn replace_expression_kind(
        &mut self,
        expression: ExpressionId,
        kind: ExpressionKind,
    ) -> ExpressionKind {
        std::mem::replace(&mut self.expressions[expression].kind, kind)
    }
}

impl Index<ExpressionId> for Storage {
    type Output = Expression;

    fn index(&self, index: ExpressionId) -> &Expression {
        &self.expressions[index]
    }
}

impl Index<PatternId> for Storage {
    type Output = Pattern;

    fn index(&self, index: PatternId) -> &Pattern {
        &self.patterns[index]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Declaration {
    pub global: Global,
    pub exported: bool,
    pub recursive_group: Option<RecursiveGroupId>,
    pub kind: DeclarationKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Value(ExpressionId),
    Constructor { arity: usize },
    Foreign,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Global {
    pub id: GlobalId,
    pub item_name: SmolStr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalId {
    Term(FileId, TermItemId),
    Instance(InstanceIdentity),
    Generated(FileId, GeneratedGlobalId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstanceIdentity {
    Declared(FileId, InstanceId),
    Derived(FileId, DeriveId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneratedGlobalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecursiveGroupId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub id: LocalId,
    pub name: SmolStr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub identity: FieldIdentity,
    pub name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldIdentity {
    Label(SmolStr),
    Member(FileId, TermItemId),
    Superclass(SuperclassIdentity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SuperclassIdentity {
    pub file_id: FileId,
    pub class: TypeItemId,
    pub source: SourceTypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(lowering::StringLiteral),
    Char(char),
    Boolean(bool),
    Integer(i32),
    Number(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionKind {
    Error,
    Literal { literal: Literal },
    Array { elements: Arc<[ExpressionId]> },
    Record { fields: Arc<[RecordField]> },
    RecordUpdate { record: ExpressionId, updates: Arc<[RecordUpdate]> },
    Project { record: ExpressionId, field: Field },
    Unary { operator: UnaryOperator, value: ExpressionId },
    Binary { operator: BinaryOperator, left: ExpressionId, right: ExpressionId },
    Constructor { global: Global },
    Global { global: Global },
    Native { operation: NativeOperation },
    Local { parameter: Parameter },
    Abstraction { parameters: Arc<[PatternId]>, body: ExpressionId },
    UncurriedAbstraction { parameters: Arc<[PatternId]>, body: ExpressionId },
    Application { function: ExpressionId, arguments: Arc<[ExpressionId]>, synthetic: bool },
    UncurriedApplication { function: ExpressionId, arguments: Arc<[ExpressionId]>, synthetic: bool },
    StyleX(StyleXExpression),
    IfThenElse { condition: ExpressionId, then: ExpressionId, else_: ExpressionId },
    Case { scrutinees: Arc<[ExpressionId]>, alternatives: Arc<[CaseAlternative]> },
    Guarded { alternatives: Arc<[GuardedAlternative]> },
    Let { recursive: bool, bindings: Arc<[Binding]>, body: ExpressionId },
    LetPattern { pattern: PatternId, value: ExpressionId, body: ExpressionId },
    Effect { effect: EffectExpression },
    SynthesizedEvidence { evidence: SynthesizedEvidence },
    TrivialEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    BooleanNot,
    IntegerNegate,
    /// Prelude's `negate` computes `0.0 - value`, preserving positive zero.
    NumberNegate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    IntegerAdd,
    IntegerSubtract,
    IntegerMultiply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectExpression {
    Pure(ExpressionId),
    Bind { action: ExpressionId, parameter: Parameter, body: ExpressionId },
    Map { function: ExpressionId, action: ExpressionId },
    Apply { function_action: ExpressionId, argument_action: ExpressionId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordField {
    pub field: Field,
    pub expression: ExpressionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordUpdate {
    Leaf { field: Field, expression: ExpressionId },
    Branch { field: Field, updates: Arc<[RecordUpdate]> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub parameter: Parameter,
    pub expression: ExpressionId,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseAlternative {
    pub patterns: Arc<[PatternId]>,
    pub expression: ExpressionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedAlternative {
    pub guards: Arc<[Guard]>,
    pub expression: ExpressionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guard {
    Boolean(ExpressionId),
    Pattern { expression: ExpressionId, pattern: PatternId },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PatternKind {
    Variable(Parameter),
    Named { parameter: Parameter, pattern: PatternId },
    Wildcard,
    Literal(Literal),
    Array(Arc<[PatternId]>),
    Record(Arc<[RecordPatternField]>),
    Constructor { global: Global, arguments: Arc<[PatternId]> },
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecordPatternField {
    pub field: Field,
    pub pattern: PatternId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthesizedEvidence {
    IsSymbol(lowering::StringLiteral),
    Reflectable(ReflectableEvidence),
    AbortIdentity(lowering::StringLiteral),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReflectableEvidence {
    Integer(i32),
    String(lowering::StringLiteral),
    Boolean(bool),
    Ordering(ReflectableOrdering),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectableOrdering {
    Less,
    Equal,
    Greater,
}
