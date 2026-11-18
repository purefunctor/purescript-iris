use smol_str::SmolStr;

use crate::core::{
    ForallBinder, ForallBinderId, RowType, RowTypeId, SmolStrId, StringLiteralId, Type, TypeFlags,
    TypeId,
};

const TYPE_CAPACITY: usize = 1 << 18;

pub struct CoreInterners {
    types: interner::parallel::Interner<Type, TypeFlags>,
    forall_binders: interner::parallel::Interner<ForallBinder>,
    row_types: interner::parallel::Interner<RowType>,
    smol_strs: interner::parallel::Interner<SmolStr>,
    string_literals: interner::parallel::Interner<lowering::StringLiteral>,
}

impl Default for CoreInterners {
    fn default() -> CoreInterners {
        CoreInterners {
            types: interner::parallel::Interner::with_capacity(TYPE_CAPACITY),
            forall_binders: interner::parallel::Interner::default(),
            row_types: interner::parallel::Interner::default(),
            smol_strs: interner::parallel::Interner::default(),
            string_literals: interner::parallel::Interner::default(),
        }
    }
}

impl CoreInterners {
    pub fn intern_type(&self, t: Type) -> TypeId {
        let id = self.types.intern_with_metadata(t, |t| self.type_flags(t));
        debug_assert_eq!(
            self.types.metadata(id),
            self.type_flags(&self.types[id]),
            "invariant violated: stored type flags differ from their recomputation",
        );
        id
    }

    #[inline]
    pub fn lookup_type(&self, id: TypeId) -> &Type {
        &self.types[id]
    }

    #[inline]
    pub fn lookup_type_flags(&self, id: TypeId) -> TypeFlags {
        self.types.metadata(id)
    }

    fn type_flags(&self, t: &Type) -> TypeFlags {
        let transitive = |id: TypeId| self.types.metadata(id).transitive();
        let expandable = |id: TypeId| {
            if self.types.metadata(id).may_expand() { TypeFlags::MAY_EXPAND } else { 0 }
        };
        let bits = match *t {
            Type::Application(left, right) | Type::KindApplication(left, right) => {
                expandable(left) | transitive(left) | transitive(right)
            }
            Type::ConstraintOrigin(left, right) => {
                TypeFlags::MAY_EXPAND | transitive(left) | transitive(right)
            }
            Type::Constrained(left, right)
            | Type::Function(left, right)
            | Type::Kinded(left, right) => transitive(left) | transitive(right),
            Type::Forall(binder_id, inner) => {
                transitive(self.forall_binders[binder_id].kind) | transitive(inner)
            }
            Type::Synonym(..) => TypeFlags::MAY_EXPAND | TypeFlags::HAS_SYNONYM,
            Type::Constructor(..) | Type::Integer(_) | Type::String(..) | Type::Free(_) => 0,
            Type::Unknown(_) => TypeFlags::HAS_UNKNOWN,
            Type::Row(row_id) => {
                let row = &self.row_types[row_id];
                let fields = row.fields.iter().fold(0, |bits, field| bits | transitive(field.id));
                let tail = row.tail.map_or(0, |tail| {
                    if matches!(self.types[tail], Type::Row(_)) {
                        TypeFlags::MAY_NORMALISE | TypeFlags::HAS_NESTED_ROW | transitive(tail)
                    } else {
                        expandable(tail) | transitive(tail)
                    }
                });
                fields | tail
            }
            Type::Rigid(name, _, kind) => {
                let scoped = if name.scoped { TypeFlags::HAS_SCOPED_RIGID } else { 0 };
                TypeFlags::HAS_RIGID | scoped | transitive(kind)
            }
            Type::Unification(_) => TypeFlags::MAY_NORMALISE | TypeFlags::HAS_UNIFICATION,
        };

        TypeFlags::from_bits(bits)
    }

    pub fn intern_forall_binder(&self, b: ForallBinder) -> ForallBinderId {
        self.forall_binders.intern(b)
    }

    #[inline]
    pub fn lookup_forall_binder(&self, id: ForallBinderId) -> ForallBinder {
        self.forall_binders[id]
    }

    pub fn intern_row_type(&self, r: RowType) -> RowTypeId {
        self.row_types.intern(r)
    }

    #[inline]
    pub fn lookup_row_type(&self, id: RowTypeId) -> &RowType {
        &self.row_types[id]
    }

    pub fn intern_smol_str(&self, s: SmolStr) -> SmolStrId {
        self.smol_strs.intern(s)
    }

    #[inline]
    pub fn lookup_smol_str(&self, id: SmolStrId) -> &SmolStr {
        &self.smol_strs[id]
    }

    pub fn intern_string_literal(&self, s: lowering::StringLiteral) -> StringLiteralId {
        self.string_literals.intern(s)
    }

    #[inline]
    pub fn lookup_string_literal(&self, id: StringLiteralId) -> &lowering::StringLiteral {
        &self.string_literals[id]
    }

    pub fn statistics(&self) -> InternerStatistics {
        let types_with_unification =
            self.types.metadata_values().filter(|flags| flags.has_unification()).count();
        InternerStatistics {
            types: self.types.count(),
            types_with_unification,
            forall_binders: self.forall_binders.count(),
            row_types: self.row_types.count(),
            smol_strs: self.smol_strs.count(),
            string_literals: self.string_literals.count(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternerStatistics {
    pub types: usize,
    pub types_with_unification: usize,
    pub forall_binders: usize,
    pub row_types: usize,
    pub smol_strs: usize,
    pub string_literals: usize,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smol_str::SmolStr;

    use super::CoreInterners;
    use crate::core::{Depth, ForallBinder, Name, RowField, RowType, Type};

    #[test]
    fn borrowed_values_remain_stable_during_concurrent_interning() {
        let interners = CoreInterners::default();
        let type_id = interners.intern_type(Type::Integer(-1));
        let row_id = interners.intern_row_type(RowType::from_closed(Arc::from([])));
        let string_id = interners.intern_smol_str(SmolStr::new("stable"));
        let borrowed_type = interners.lookup_type(type_id);
        let borrowed_row = interners.lookup_row_type(row_id);
        let borrowed_string = interners.lookup_smol_str(string_id);

        std::thread::scope(|scope| {
            for _ in 0..4 {
                let interners = &interners;
                scope.spawn(move || {
                    for value in 0..4096 {
                        let id = interners.intern_type(Type::Integer(value));
                        let fields = Arc::from([RowField { label: SmolStr::new("field"), id }]);
                        interners.intern_row_type(RowType::from_closed(fields));
                        interners.intern_smol_str(SmolStr::new(value.to_string()));
                        assert_eq!(borrowed_type, &Type::Integer(-1));
                        assert_eq!(borrowed_row, &RowType::from_closed(Arc::from([])));
                        assert_eq!(borrowed_string, "stable");
                    }
                });
            }
        });

        assert!(std::ptr::eq(borrowed_type, interners.lookup_type(type_id)));
        assert!(std::ptr::eq(borrowed_row, interners.lookup_row_type(row_id)));
        assert!(std::ptr::eq(borrowed_string, interners.lookup_smol_str(string_id)));
    }

    #[test]
    fn normalisation_flags_describe_head_reductions() {
        let interners = CoreInterners::default();

        let integer = interners.intern_type(Type::Integer(0));
        let unification = interners.intern_type(Type::Unification(0));
        let application = interners.intern_type(Type::Application(integer, unification));

        assert!(!interners.lookup_type_flags(integer).may_normalise());
        assert!(interners.lookup_type_flags(unification).may_normalise());
        assert!(!interners.lookup_type_flags(application).may_normalise());
        let closed_row = RowType::from_closed(Arc::from([RowField {
            label: SmolStr::new("inner"),
            id: integer,
        }]));
        let closed_row = interners.intern_row_type(closed_row);
        let closed_row = interners.intern_type(Type::Row(closed_row));

        let nested_row = RowType::from_open(
            Arc::from([RowField { label: SmolStr::new("outer"), id: integer }]),
            closed_row,
        );
        let nested_row = interners.intern_row_type(nested_row);
        let nested_row = interners.intern_type(Type::Row(nested_row));

        assert!(!interners.lookup_type_flags(closed_row).may_normalise());
        assert!(interners.lookup_type_flags(nested_row).may_normalise());
    }

    #[test]
    fn transitive_flags_describe_descendants() {
        let interners = CoreInterners::default();
        let file = files::Files::default().insert("Main.purs", "");

        let integer = interners.intern_type(Type::Integer(0));
        let unification = interners.intern_type(Type::Unification(0));
        let name = Name { file, unique: 0, scoped: false };
        let rigid = interners.intern_type(Type::Rigid(name, Depth(0), integer));

        let closed = interners.intern_type(Type::Function(integer, integer));
        assert!(!interners.lookup_type_flags(closed).may_zonk());
        assert!(!interners.lookup_type_flags(closed).may_substitute());

        let with_rigid = interners.intern_type(Type::Function(integer, rigid));
        let with_rigid = interners.intern_type(Type::Application(with_rigid, integer));
        assert!(!interners.lookup_type_flags(with_rigid).may_zonk());
        assert!(interners.lookup_type_flags(with_rigid).may_substitute());

        let row = RowType::from_closed(Arc::from([RowField {
            label: SmolStr::new("field"),
            id: unification,
        }]));
        let row = interners.intern_row_type(row);
        let with_unification = interners.intern_type(Type::Row(row));
        assert!(!interners.lookup_type_flags(with_unification).may_normalise());
        assert!(interners.lookup_type_flags(with_unification).may_zonk());
        assert!(interners.lookup_type_flags(with_unification).may_substitute());

        let inner_row = RowType::from_closed(Arc::from([]));
        let inner_row = interners.intern_row_type(inner_row);
        let inner_row = interners.intern_type(Type::Row(inner_row));
        let nested_row = RowType::from_open(
            Arc::from([RowField { label: SmolStr::new("outer"), id: integer }]),
            inner_row,
        );
        let nested_row = interners.intern_row_type(nested_row);
        let nested_row = interners.intern_type(Type::Row(nested_row));
        let with_nested_row = interners.intern_type(Type::Kinded(integer, nested_row));
        assert!(!interners.lookup_type_flags(with_nested_row).may_normalise());
        assert!(interners.lookup_type_flags(with_nested_row).may_zonk());
    }

    #[test]
    fn scoped_rigid_flags_include_kinds_and_rows() {
        let interners = CoreInterners::default();
        let file = files::Files::default().insert("Main.purs", "");
        let plain = interners.intern_type(Type::Integer(0));
        let scoped_name = Name { file, unique: 0, scoped: true };
        let unscoped_name = Name { file, unique: 1, scoped: false };
        let scoped = interners.intern_type(Type::Rigid(scoped_name, Depth(0), plain));
        let unscoped = interners.intern_type(Type::Rigid(unscoped_name, Depth(0), plain));
        let rigid_kind = interners.intern_type(Type::Rigid(unscoped_name, Depth(0), scoped));

        let binder = ForallBinder { name: unscoped_name, kind: scoped, visible: false };
        let binder = interners.intern_forall_binder(binder);
        let binder_kind = interners.intern_type(Type::Forall(binder, unscoped));

        let field = RowField { label: SmolStr::new("field"), id: scoped };
        let row = interners.intern_row_type(RowType::from_closed(Arc::from([field])));
        let row_field = interners.intern_type(Type::Row(row));
        let field = RowField { label: SmolStr::new("outer"), id: unscoped };
        let fields = Arc::from([field]);
        let scoped_tail = RowType::from_open(Arc::clone(&fields), scoped);
        let scoped_tail = interners.intern_row_type(scoped_tail);
        let scoped_tail = interners.intern_type(Type::Row(scoped_tail));
        let nested_tail = RowType::from_open(fields, row_field);
        let nested_tail = interners.intern_row_type(nested_tail);
        let nested_tail = interners.intern_type(Type::Row(nested_tail));

        for id in [scoped, rigid_kind, binder_kind, row_field, scoped_tail, nested_tail] {
            assert!(
                interners.lookup_type_flags(id).has_scoped_rigid(),
                "{:?}",
                interners.lookup_type(id)
            );
        }
        assert!(!interners.lookup_type_flags(unscoped).has_scoped_rigid());
    }
}
