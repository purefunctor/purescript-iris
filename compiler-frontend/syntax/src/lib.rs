pub mod ast;
pub mod cst;
pub mod names;
mod token_set;
mod tree;

pub use token_set::TokenSet;
pub use tree::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // Generic
    TEXT = 0,

    // Names
    HOLE,
    UPPER,
    LOWER,
    OPERATOR,
    OPERATOR_NAME,

    // Operators
    AT,
    BACKSLASH,
    COLON,
    DOUBLE_COLON,
    COMMA,
    EQUAL,
    MINUS,
    PERIOD,
    DOUBLE_PERIOD,
    DOUBLE_PERIOD_OPERATOR_NAME,
    PIPE,
    TICK,
    UNDERSCORE,
    LEFT_ARROW,
    RIGHT_ARROW,
    LEFT_THICK_ARROW,
    RIGHT_THICK_ARROW,
    LEFT_PARENTHESIS,
    RIGHT_PARENTHESIS,
    LEFT_CURLY,
    RIGHT_CURLY,
    LEFT_SQUARE,
    RIGHT_SQUARE,

    // Keywords
    ADO,
    AS,
    CASE,
    CLASS,
    DATA,
    DERIVE,
    DO,
    ELSE,
    FORALL,
    FOREIGN,
    HIDING,
    IF,
    IMPORT,
    IN,
    INFIX,
    INFIXL,
    INFIXR,
    INSTANCE,
    LET,
    MODULE,
    NEWTYPE,
    NOMINAL,
    OF,
    PHANTOM,
    REPRESENTATIONAL,
    ROLE,
    THEN,
    TYPE,
    WHERE,

    // Literals
    CHAR,
    INTEGER,
    NUMBER,
    RAW_STRING,
    STRING,
    TRUE,
    FALSE,

    // Control
    LAYOUT_START,
    LAYOUT_SEPARATOR,
    LAYOUT_END,
    END_OF_FILE,

    // Special
    ERROR,

    // Nodes
    Node,
    Annotation,
    Qualifier,

    ModuleName,
    QualifiedName,
    LabelName,

    Module,
    ModuleHeader,
    ExportList,
    ExportValue,
    ExportClass,
    ExportType,
    ExportOperator,
    ExportTypeOperator,
    ExportModule,
    ModuleImports,
    ImportStatement,
    ImportList,
    ImportValue,
    ImportClass,
    ImportType,
    ImportOperator,
    ImportTypeOperator,
    ImportAlias,
    ModuleStatements,

    TypeItemsAll,
    TypeItemsList,

    ValueSignature,
    ValueEquation,
    FunctionBinders,

    Unconditional,
    WhereExpression,

    Conditionals,
    PatternGuarded,
    PatternGuardBinder,
    PatternGuardExpression,

    LetBindingSignature,
    LetBindingEquation,
    LetBindingPattern,
    LetBindingStatements,

    TypeApplicationChain,
    TypeArrow,
    TypeConstrained,
    TypeConstructor,
    TypeForall,
    TypeHole,
    TypeInteger,
    TypeKinded,
    TypeOperatorName,
    TypeOperatorChain,
    TypeOperatorPair,
    TypeString,
    TypeVariable,
    TypeVariableBinding,
    TypeWildcard,
    TypeEffectSet,
    TypeEffectSetMember,
    TypeEffectSetTail,
    TypeRecord,
    TypeRow,
    TypeRowItem,
    TypeRowTail,
    TypeParenthesized,

    BinderTyped,
    BinderOperatorChain,
    BinderOperatorPair,
    BinderInteger,
    BinderNumber,
    BinderConstructor,
    BinderVariable,
    BinderNamed,
    BinderWildcard,
    BinderString,
    BinderChar,
    BinderTrue,
    BinderFalse,
    BinderArray,
    BinderRecord,
    BinderParenthesized,

    ExpressionTyped,
    ExpressionOperatorChain,
    ExpressionOperatorPair,
    ExpressionInfixChain,
    ExpressionInfixPair,
    ExpressionTick,
    ExpressionNegate,
    ExpressionApplicationChain,
    ExpressionTypeArgument,
    ExpressionTermArgument,
    ExpressionIfThenElse,
    ExpressionIf,
    ExpressionThen,
    ExpressionElse,
    ExpressionLetIn,
    ExpressionLambda,
    ExpressionCaseOf,
    ExpressionDo,
    ExpressionAdo,
    ExpressionConstructor,
    ExpressionVariable,
    ExpressionOperatorName,
    ExpressionSection,
    ExpressionHole,
    ExpressionString,
    ExpressionChar,
    ExpressionTrue,
    ExpressionFalse,
    ExpressionInteger,
    ExpressionNumber,
    ExpressionArray,
    ExpressionRecord,
    ExpressionParenthesized,
    ExpressionRecordAccess,
    ExpressionRecordUpdate,

    CaseTrunk,
    CaseBranches,
    CaseBranchBinders,
    CaseBranch,

    DoStatements,
    DoStatementBind,
    DoStatementLet,
    DoStatementDiscard,

    RecordAccessLabel,
    RecordField,
    RecordPun,
    RecordUpdates,
    RecordUpdateLeaf,
    RecordUpdateBranch,

    InfixDeclaration,

    TypeRoleDeclaration,
    TypeRole,

    TypeSynonymSignature,
    TypeSynonymEquation,

    ClassSignature,
    ClassDeclaration,
    ClassConstraints,
    ClassHead,
    ClassFunctionalDependencies,
    FunctionalDependencyDetermined,
    FunctionalDependencyDetermines,
    ClassStatements,
    ClassMemberStatement,

    InstanceChain,
    InstanceDeclaration,
    InstanceName,
    InstanceConstraints,
    InstanceHead,
    InstanceStatements,
    InstanceSignatureStatement,
    InstanceEquationStatement,

    ForeignImportDataDeclaration,
    ForeignImportValueDeclaration,

    NewtypeSignature,
    NewtypeEquation,

    DataSignature,
    DataEquation,
    DataConstructor,

    DeriveDeclaration,

    TermOperator,
    TypeOperator,
}

impl SyntaxKind {
    pub fn is_layout_token(&self) -> bool {
        matches!(self, Self::LAYOUT_START | Self::LAYOUT_SEPARATOR | Self::LAYOUT_END)
    }
}
