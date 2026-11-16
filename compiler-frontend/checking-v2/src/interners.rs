use interner::parallel::Interner;
use smol_str::SmolStr;

use crate::types::{
    ForallBinder, ForallBinderId, Rigid, RigidId, RowType, RowTypeId, SynonymApplication,
    SynonymApplicationId, TextId, Type, TypeFlags, TypeId,
};

#[derive(Default)]
pub struct Interners {
    types: Interner<Type, TypeFlags>,
    forall_binders: Interner<ForallBinder>,
    rigids: Interner<Rigid>,
    row_types: Interner<RowType>,
    synonym_applications: Interner<SynonymApplication>,
    texts: Interner<SmolStr>,
}

impl Interners {
    pub fn intern_type(&self, t: Type) -> TypeId {
        let flags = self.type_flags(&t);
        self.types.intern_with_metadata(t, flags)
    }

    pub fn lookup_type(&self, id: TypeId) -> Type {
        self.types[id]
    }

    pub fn lookup_type_flags(&self, id: TypeId) -> TypeFlags {
        self.types.metadata(id)
    }

    pub fn intern_forall_binder(&self, binder: ForallBinder) -> ForallBinderId {
        self.forall_binders.intern(binder)
    }

    pub fn lookup_forall_binder(&self, id: ForallBinderId) -> ForallBinder {
        self.forall_binders[id]
    }

    pub fn intern_rigid(&self, rigid: Rigid) -> RigidId {
        self.rigids.intern(rigid)
    }

    pub fn lookup_rigid(&self, id: RigidId) -> Rigid {
        self.rigids[id]
    }

    pub fn intern_row_type(&self, row: RowType) -> RowTypeId {
        self.row_types.intern(row)
    }

    pub fn lookup_row_type(&self, id: RowTypeId) -> &RowType {
        &self.row_types[id]
    }

    pub fn intern_synonym_application(
        &self,
        application: SynonymApplication,
    ) -> SynonymApplicationId {
        self.synonym_applications.intern(application)
    }

    pub fn lookup_synonym_application(&self, id: SynonymApplicationId) -> &SynonymApplication {
        &self.synonym_applications[id]
    }

    pub fn intern_text(&self, text: SmolStr) -> TextId {
        self.texts.intern(text)
    }

    pub fn lookup_text(&self, id: TextId) -> &SmolStr {
        &self.texts[id]
    }

    fn type_flags(&self, t: &Type) -> TypeFlags {
        let transitive = |id: TypeId| self.types.metadata(id).bits();
        let bits = match *t {
            Type::Application(function, argument) | Type::KindApplication(function, argument) => {
                transitive(function) | transitive(argument)
            }
            Type::Function(argument, result) => transitive(argument) | transitive(result),
            Type::Constrained(constraint, constrained) => {
                transitive(constraint) | transitive(constrained)
            }
            Type::Kinded(inner, kind) => transitive(inner) | transitive(kind),
            Type::Forall(binder_id, inner) => {
                let binder = self.forall_binders[binder_id];
                transitive(binder.kind) | transitive(inner)
            }
            Type::Constructor(..) | Type::Integer(_) | Type::String(_) => 0,
            Type::Synonym(application_id) => {
                let application = &self.synonym_applications[application_id];
                let arguments = application.arguments.iter();
                let arguments = arguments.fold(0, |bits, &argument| bits | transitive(argument));
                TypeFlags::HAS_SYNONYM | arguments | transitive(application.expansion)
            }
            Type::Row(row_id) => {
                let row = &self.row_types[row_id];
                let fields = row.fields.iter().fold(0, |bits, field| bits | transitive(field.id));
                let tail = row.tail.map_or(0, |tail| {
                    let nested = matches!(self.types[tail], Type::Row(_));
                    let nested = if nested { TypeFlags::HAS_NESTED_ROW } else { 0 };
                    nested | transitive(tail)
                });
                fields | tail
            }
            Type::Variable(binder_id) => {
                let binder = self.forall_binders[binder_id];
                TypeFlags::HAS_VARIABLE | transitive(binder.kind)
            }
            Type::Rigid(rigid_id) => {
                let rigid = self.rigids[rigid_id];
                TypeFlags::HAS_RIGID | transitive(rigid.kind)
            }
            Type::Unification(_) => TypeFlags::HAS_UNIFICATION,
            Type::Unknown => TypeFlags::HAS_UNKNOWN,
        };
        TypeFlags::from_bits(bits)
    }
}
