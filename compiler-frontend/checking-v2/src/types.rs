use std::sync::Arc;

use files::FileId;
use indexing::TypeItemId;
use smol_str::SmolStr;

pub type TypeId = interner::Id<Type>;
pub type ForallBinderId = interner::Id<ForallBinder>;
pub type RigidId = interner::Id<Rigid>;
pub type RowTypeId = interner::Id<RowType>;
pub type SynonymApplicationId = interner::Id<SynonymApplication>;
pub type TextId = interner::Id<SmolStr>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name {
    pub file: FileId,
    pub unique: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(pub u32);

impl Level {
    pub fn increment(self) -> Level {
        Level(self.0 + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnificationId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForallBinder {
    pub visible: bool,
    pub name: Name,
    pub text: TextId,
    pub kind: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rigid {
    pub name: Name,
    pub text: TextId,
    pub level: Level,
    pub kind: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowField {
    pub label: SmolStr,
    pub id: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowType {
    pub fields: Arc<[RowField]>,
    pub tail: Option<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SynonymApplication {
    pub file_id: FileId,
    pub type_id: TypeItemId,
    pub arguments: Arc<[TypeId]>,
    pub expansion: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Application(TypeId, TypeId),
    KindApplication(TypeId, TypeId),
    Function(TypeId, TypeId),
    Forall(ForallBinderId, TypeId),
    Constrained(TypeId, TypeId),
    Kinded(TypeId, TypeId),
    Constructor(FileId, TypeItemId),
    Synonym(SynonymApplicationId),
    Integer(i32),
    String(TextId),
    Row(RowTypeId),
    Variable(ForallBinderId),
    Rigid(RigidId),
    Unification(UnificationId),
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeFlags(u8);

impl TypeFlags {
    pub const HAS_UNIFICATION: u8 = 1 << 0;
    pub const HAS_RIGID: u8 = 1 << 1;
    pub const HAS_VARIABLE: u8 = 1 << 2;
    pub const HAS_SYNONYM: u8 = 1 << 3;
    pub const HAS_UNKNOWN: u8 = 1 << 4;
    pub const HAS_NESTED_ROW: u8 = 1 << 5;

    pub fn from_bits(bits: u8) -> TypeFlags {
        TypeFlags(bits)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn has_unification(self) -> bool {
        self.0 & TypeFlags::HAS_UNIFICATION != 0
    }

    pub fn has_rigid(self) -> bool {
        self.0 & TypeFlags::HAS_RIGID != 0
    }

    pub fn has_variable(self) -> bool {
        self.0 & TypeFlags::HAS_VARIABLE != 0
    }

    pub fn has_unknown(self) -> bool {
        self.0 & TypeFlags::HAS_UNKNOWN != 0
    }

    pub fn may_zonk(self) -> bool {
        self.0 & (TypeFlags::HAS_UNIFICATION | TypeFlags::HAS_NESTED_ROW) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Phantom,
    Representational,
    Nominal,
}
