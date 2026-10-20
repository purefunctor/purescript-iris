use stabilizing::AstId;
use syntax::cst;

pub type BinderId = AstId<cst::Binder>;
pub type ExpressionId = AstId<cst::Expression>;
pub type RecordAccessLabelId = AstId<cst::RecordAccessLabel>;
pub type RecordFieldId = AstId<cst::RecordField>;
pub type RecordPunId = AstId<cst::RecordPun>;
pub type RecordUpdateId = AstId<cst::RecordUpdate>;

pub type TypeId = AstId<cst::Type>;
pub type TypeVariableBindingId = AstId<cst::TypeVariableBinding>;

pub type CaseBranchId = AstId<cst::CaseBranch>;
pub type DoStatementId = AstId<cst::DoStatement>;
pub type LetBindingId = AstId<cst::LetBindingPattern>;
pub type PatternGuardId = AstId<cst::PatternGuard>;

pub type LetBindingSignatureId = AstId<cst::LetBindingSignature>;
pub type LetBindingEquationId = AstId<cst::LetBindingEquation>;

pub type InstanceMemberId = AstId<cst::InstanceMemberStatement>;
pub type InstanceSignatureId = AstId<cst::InstanceSignatureStatement>;
pub type InstanceEquationId = AstId<cst::InstanceEquationStatement>;

pub type TermOperatorId = AstId<cst::TermOperator>;
pub type TypeOperatorId = AstId<cst::TypeOperator>;
