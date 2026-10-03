use std::sync::Arc;

use indexing::TypeItemId;
use smol_str::SmolStr;
use stabilizing::AstId;
use syntax::cst;

#[derive(Debug, PartialEq, Eq)]
pub enum LoweringError {
    NotInScope(NotInScope),
    InvalidStringEscape { source: StringLiteralSource },
    JsxTagMismatch { id: AstId<cst::JsxClosing>, expected: Option<SmolStr> },
    DuplicateJsxAttribute { id: AstId<cst::JsxAttribute>, name: SmolStr },
    RecursiveSynonym(RecursiveGroup),
    RecursiveKinds(RecursiveGroup),
}

#[derive(Debug, PartialEq, Eq)]
pub enum StringLiteralSource {
    Expression(AstId<cst::ExpressionString>),
    Binder(AstId<cst::BinderString>),
    Type(AstId<cst::TypeString>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum NotInScope {
    ExprConstructor { id: AstId<cst::ExpressionConstructor> },
    ExprVariable { id: AstId<cst::ExpressionVariable> },
    ExprOperatorName { id: AstId<cst::ExpressionOperatorName> },
    TypeClass { id: AstId<cst::InstanceHead> },
    TypeConstructor { id: AstId<cst::TypeConstructor> },
    TypeVariable { id: AstId<cst::TypeVariable> },
    TypeOperatorName { id: AstId<cst::TypeOperatorName> },
    DoFn { kind: DoFn, id: AstId<cst::ExpressionDo> },
    AdoFn { kind: AdoFn, id: AstId<cst::ExpressionAdo> },
    NegateFn { id: AstId<cst::ExpressionNegate> },
    TermOperator { id: AstId<cst::TermOperator> },
    TypeOperator { id: AstId<cst::TypeOperator> },
    JsxFunction { id: crate::ExpressionId, name: SmolStr },
}

#[derive(Debug, PartialEq, Eq)]
pub enum DoFn {
    Bind,
    Discard,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AdoFn {
    Map,
    Apply,
    Pure,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecursiveGroup {
    pub group: Arc<[TypeItemId]>,
}
