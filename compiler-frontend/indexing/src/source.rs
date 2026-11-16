use stabilizing::AstId;
use syntax::cst;

pub type ExportItemId = AstId<cst::ExportItem>;
pub type ImportItemId = AstId<cst::ImportItem>;

pub type ImportId = AstId<cst::ImportStatement>;
pub type DeclarationId = AstId<cst::Declaration>;

pub type DataSignatureId = AstId<cst::DataSignature>;
pub type DataEquationId = AstId<cst::DataEquation>;
pub type DataConstructorId = AstId<cst::DataConstructor>;

pub type NewtypeSignatureId = AstId<cst::NewtypeSignature>;
pub type NewtypeEquationId = AstId<cst::NewtypeEquation>;

pub type TypeRoleId = AstId<cst::TypeRoleDeclaration>;
pub type TypeSignatureId = AstId<cst::TypeSynonymSignature>;
pub type TypeEquationId = AstId<cst::TypeSynonymEquation>;

pub type ClassSignatureId = AstId<cst::ClassSignature>;
pub type ClassDeclarationId = AstId<cst::ClassDeclaration>;
pub type ClassMemberId = AstId<cst::ClassMemberStatement>;

pub type ValueSignatureId = AstId<cst::ValueSignature>;
pub type ValueEquationId = AstId<cst::ValueEquation>;

pub type InstanceChainId = AstId<cst::InstanceChain>;
pub type InstanceId = AstId<cst::InstanceDeclaration>;
pub type InstanceMemberId = AstId<cst::InstanceMemberStatement>;
pub type InstanceEquationId = AstId<cst::InstanceEquationStatement>;
pub type DeriveId = AstId<cst::Derive>;

pub type InfixId = AstId<cst::InfixDeclaration>;
pub type ForeignDataId = AstId<cst::ForeignImportDataDeclaration>;
pub type ForeignValueId = AstId<cst::ForeignImportValueDeclaration>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquationSourceId {
    Value(ValueEquationId),
    Instance(InstanceEquationId),
}
