use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct Package {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub description: Option<String>,
    pub dependencies: BTreeMap<String, String>,
    pub modules: Vec<String>,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum Location {
    #[serde(rename = "github")]
    #[ts(rename = "github")]
    GitHub {
        url: String,
        owner: String,
        repository: String,
        reference: Option<String>,
        subdir: Option<String>,
    },
    #[serde(rename = "git")]
    #[ts(rename = "git")]
    Git { url: String, reference: Option<String>, subdir: Option<String> },
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct Module {
    pub name: String,
    pub documentation: Option<String>,
    pub terms: Vec<TermItem>,
    pub types: Vec<TypeItem>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TermItem {
    pub name: Option<String>,
    pub documentation: Option<String>,
    pub signature: Option<Type>,
    pub kind: TermKind,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub enum TermKind {
    ClassMember,
    Constructor,
    Derive,
    Foreign,
    Instance,
    Operator,
    Value,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TypeItem {
    pub name: Option<String>,
    pub documentation: Option<String>,
    pub form: TypeItemForm,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum TypeItemForm {
    Data {
        signature: Option<Type>,
        declaration: Option<TypeDeclaration>,
        constructors: Vec<TermItem>,
        instances: Vec<TermItem>,
    },
    Newtype {
        signature: Option<Type>,
        declaration: Option<TypeDeclaration>,
        constructors: Vec<TermItem>,
        instances: Vec<TermItem>,
    },
    Synonym {
        signature: Option<Type>,
        equation: Option<TypeSynonymEquation>,
        instances: Vec<TermItem>,
    },
    Class {
        signature: Option<Type>,
        declaration: Option<TypeDeclaration>,
        superclasses: Vec<Type>,
        #[serde(rename = "functionalDependencies")]
        functional_dependencies: Vec<FunctionalDependency>,
        members: Vec<TermItem>,
        instances: Vec<TermItem>,
    },
    Foreign {
        signature: Option<Type>,
        instances: Vec<TermItem>,
    },
    Operator {
        signature: Option<Type>,
    },
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct FunctionalDependency {
    pub determiners: Vec<String>,
    pub determined: Vec<String>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TypeDeclaration {
    pub binders: Vec<TypeBinder>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TypeSynonymEquation {
    pub binders: Vec<TypeBinder>,
    pub expansion: Type,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum Type {
    Application { function: Box<Type>, argument: Box<Type> },
    KindApplication { function: Box<Type>, argument: Box<Type> },
    Forall { binder: TypeBinder, body: Box<Type> },
    Constrained { constraint: Box<Type>, body: Box<Type> },
    Function { argument: Box<Type>, result: Box<Type> },
    Kinded { expression: Box<Type>, kind: Box<Type> },
    Constructor { reference: TypeReference },
    Integer { value: i32 },
    String { kind: StringLiteralKind, value: StringLiteralValue },
    Row { fields: Vec<TypeRowField>, tail: Option<Box<Type>> },
    Rigid { name: String, kind: Box<Type> },
    Unification { id: u32 },
    Free { name: String },
    Unknown { name: String },
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TypeReference {
    pub package: Option<String>,
    pub module: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TypeBinder {
    pub name: String,
    pub visible: bool,
    pub kind: Box<Type>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
pub struct TypeRowField {
    pub label: String,
    #[serde(rename = "type")]
    pub t: Type,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
#[serde(rename_all = "camelCase")]
pub enum StringLiteralKind {
    String,
    RawString,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export_to = "docs-schema.ts")]
#[serde(untagged)]
pub enum StringLiteralValue {
    Utf8(String),
    Utf16(Vec<u16>),
}

impl From<lowering::StringKind> for StringLiteralKind {
    fn from(kind: lowering::StringKind) -> StringLiteralKind {
        match kind {
            lowering::StringKind::String => StringLiteralKind::String,
            lowering::StringKind::RawString => StringLiteralKind::RawString,
        }
    }
}
