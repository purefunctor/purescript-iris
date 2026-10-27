//! Arena-allocated functional trees produced from checked modules.

use std::ops::Index;
use std::sync::Arc;

use files::FileId;
use indexing::{
    ClassMemberId, DataConstructorId, DeriveId, EquationSourceId, ForeignValueId, ImportId,
    InstanceId, TermItemId, TypeItemId,
};
use la_arena::{Arena, Idx};
use lowering::TypeId as SourceTypeId;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use stabilizing::StabilizedModule;
use syntax::SyntaxNodePtr;

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
    /// Imported `defineVars`, `defineConsts`, and `defineMarker` values that StyleX evaluates
    /// statically, which it resolves only through imports of the defining module's theme file.
    pub stylex_theme_imports: FxHashSet<GlobalId>,
    pub stylex_import: Option<ImportId>,
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
    pub import: Option<ImportId>,
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
    ) {
        self.expressions[expression].kind = kind;
    }

    pub(crate) fn replace_expression(
        &mut self,
        expression: ExpressionId,
        replacement: ExpressionId,
    ) {
        let Expression { kind, origin } = Expression::clone(&self.expressions[replacement]);
        let expression = &mut self.expressions[expression];
        expression.kind = kind;
        expression.origin = origin.or(expression.origin);
    }

    pub(crate) fn set_pattern_origin(&mut self, pattern: PatternId, origin: Origin) {
        self.patterns[pattern].origin = Some(origin);
    }

    pub(crate) fn relocate_expression(
        &mut self,
        expression: ExpressionId,
        replacement: ExpressionKind,
    ) -> ExpressionId {
        let origin = self.expressions[expression].origin;
        let kind = std::mem::replace(&mut self.expressions[expression].kind, replacement);
        self.expressions.alloc(Expression { kind, origin })
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
    pub origin: Option<Origin>,
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
pub enum Origin {
    Expression(lowering::ExpressionId),
    CaseBranch(lowering::CaseBranchId),
    PatternGuard(lowering::PatternGuardId),
    DoStatement(lowering::DoStatementId),
    Equation(EquationSourceId),
    LocalEquation(lowering::LetBindingEquationId),
    LetPattern(lowering::LetBindingId),
    Constructor(DataConstructorId),
    Foreign(ForeignValueId),
    ClassMember(ClassMemberId),
    Instance(InstanceId),
    Derive(DeriveId),
    Import(ImportId),
}

impl Origin {
    pub fn syntax_ptr(self, stabilized: &StabilizedModule) -> Option<SyntaxNodePtr> {
        match self {
            Origin::Expression(id) => stabilized.syntax_ptr(id),
            Origin::CaseBranch(id) => stabilized.syntax_ptr(id),
            Origin::PatternGuard(id) => stabilized.syntax_ptr(id),
            Origin::DoStatement(id) => stabilized.syntax_ptr(id),
            Origin::Equation(EquationSourceId::Value(id)) => stabilized.syntax_ptr(id),
            Origin::Equation(EquationSourceId::Instance(id)) => stabilized.syntax_ptr(id),
            Origin::LocalEquation(id) => stabilized.syntax_ptr(id),
            Origin::LetPattern(id) => stabilized.syntax_ptr(id),
            Origin::Constructor(id) => stabilized.syntax_ptr(id),
            Origin::Foreign(id) => stabilized.syntax_ptr(id),
            Origin::ClassMember(id) => stabilized.syntax_ptr(id),
            Origin::Instance(id) => stabilized.syntax_ptr(id),
            Origin::Derive(id) => stabilized.syntax_ptr(id),
            Origin::Import(id) => stabilized.syntax_ptr(id),
        }
    }
}

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
    Integer(i64),
    Number(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub origin: Option<Origin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionKind {
    Error,
    Literal { literal: Literal },
    Array { elements: Arc<[ExpressionId]> },
    ArrayConcatenation { elements: Arc<[ArrayElement]> },
    Record { fields: Arc<[RecordField]> },
    RecordUpdate { record: ExpressionId, updates: Arc<[RecordUpdate]> },
    RecordUnion { left: ExpressionId, right: ExpressionId },
    RecordDelete { record: ExpressionId, field: Field },
    Project { record: ExpressionId, field: Field },
    Unary { operator: UnaryOperator, value: ExpressionId },
    Binary { operator: BinaryOperator, left: ExpressionId, right: ExpressionId },
    Index { value: ExpressionId, index: ExpressionId },
    Constructor { global: Global },
    Global { global: Global },
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
pub enum ArrayElement {
    Value(ExpressionId),
    Spread(ExpressionId),
}

impl ArrayElement {
    pub fn expression(self) -> ExpressionId {
        match self {
            ArrayElement::Value(expression) | ArrayElement::Spread(expression) => expression,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    BooleanNot,
    IntegerNegate,
    IntegerComplement,
    NumberNegate,
    ArrayLength,
    ObjectKeys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    BooleanAnd,
    BooleanOr,
    IntegerAdd,
    IntegerSubtract,
    IntegerMultiply,
    IntegerDivide,
    IntegerModulo,
    IntegerAnd,
    IntegerOr,
    IntegerXor,
    IntegerShiftLeft,
    IntegerShiftRight,
    IntegerZeroFillShiftRight,
    NumberAdd,
    NumberSubtract,
    NumberMultiply,
    NumberDivide,
    StringAppend,
    ObjectMember,
    Comparison { operand: PrimitiveType, comparison: Comparison },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Boolean,
    Integer,
    Number,
    Char,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectExpression {
    Pure(ExpressionId),
    Bind { action: ExpressionId, parameter: Parameter, body: ExpressionId },
    Map { function: ExpressionId, action: ExpressionId },
    Apply { function_action: ExpressionId, argument_action: ExpressionId },
    Primitive(EffectPrimitive),
    Loop(EffectLoop),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectPrimitive {
    NewReference {
        value: ExpressionId,
    },
    ReadReference {
        reference: ExpressionId,
    },
    WriteReference {
        value: ExpressionId,
        reference: ExpressionId,
        result: ReferenceWriteResult,
    },
    ModifyReference {
        function: ExpressionId,
        reference: ExpressionId,
        result: ReferenceModifyResult,
    },
    UncurriedCall {
        function: ExpressionId,
        arguments: Arc<[ExpressionId]>,
    },
    NewArray,
    InsertArrayValues {
        values: ExpressionId,
        array: ExpressionId,
        end: ArrayEnd,
    },
    CopyArray {
        array: ExpressionId,
    },
    ReadArrayLength {
        array: ExpressionId,
    },
    ReadArrayValue {
        index: ExpressionId,
        array: ExpressionId,
    },
    WriteArrayValue {
        index: ExpressionId,
        value: ExpressionId,
        array: ExpressionId,
    },
    NewObject,
    WriteObjectProperty {
        key: ExpressionId,
        value: ExpressionId,
        object: ExpressionId,
    },
    DeleteObjectProperty {
        key: ExpressionId,
        object: ExpressionId,
    },
    CopyObject {
        object: ExpressionId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayEnd {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceWriteResult {
    Unit,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceModifyResult {
    Unit,
    State,
    Record,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectLoop {
    Range { start: ExpressionId, end: ExpressionId, body: ExpressionId },
    Each { array: ExpressionId, body: ExpressionId },
    While { condition: ExpressionId, body: ExpressionId },
    Until { condition: ExpressionId },
}

impl EffectPrimitive {
    pub fn operands(&self) -> impl Iterator<Item = ExpressionId> + '_ {
        let (fixed, rest): ([Option<ExpressionId>; 3], &[ExpressionId]) = match self {
            EffectPrimitive::NewReference { value } => ([Some(*value), None, None], &[]),
            EffectPrimitive::ReadReference { reference } => ([Some(*reference), None, None], &[]),
            EffectPrimitive::WriteReference { value, reference, .. } => {
                ([Some(*value), Some(*reference), None], &[])
            }
            EffectPrimitive::ModifyReference { function, reference, .. } => {
                ([Some(*function), Some(*reference), None], &[])
            }
            EffectPrimitive::UncurriedCall { function, arguments } => {
                ([Some(*function), None, None], arguments)
            }
            EffectPrimitive::NewArray | EffectPrimitive::NewObject => ([None, None, None], &[]),
            EffectPrimitive::InsertArrayValues { values, array, .. } => {
                ([Some(*values), Some(*array), None], &[])
            }
            EffectPrimitive::CopyArray { array } | EffectPrimitive::ReadArrayLength { array } => {
                ([Some(*array), None, None], &[])
            }
            EffectPrimitive::ReadArrayValue { index, array } => {
                ([Some(*index), Some(*array), None], &[])
            }
            EffectPrimitive::WriteArrayValue { index, value, array } => {
                ([Some(*index), Some(*value), Some(*array)], &[])
            }
            EffectPrimitive::WriteObjectProperty { key, value, object } => {
                ([Some(*key), Some(*value), Some(*object)], &[])
            }
            EffectPrimitive::DeleteObjectProperty { key, object } => {
                ([Some(*key), Some(*object), None], &[])
            }
            EffectPrimitive::CopyObject { object } => ([Some(*object), None, None], &[]),
        };
        fixed.into_iter().flatten().chain(rest.iter().copied())
    }
}

impl EffectLoop {
    pub fn operands(&self) -> impl Iterator<Item = ExpressionId> {
        let (first, second, third) = match self {
            EffectLoop::Range { start, end, body } => (*start, Some(*end), Some(*body)),
            EffectLoop::Each { array, body } => (*array, Some(*body), None),
            EffectLoop::While { condition, body } => (*condition, Some(*body), None),
            EffectLoop::Until { condition } => (*condition, None, None),
        };
        std::iter::once(first).chain(second).chain(third)
    }
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
    pub origin: Option<Origin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseAlternative {
    pub patterns: Arc<[PatternId]>,
    pub expression: ExpressionId,
    pub origin: Option<Origin>,
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
    pub origin: Option<Origin>,
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
