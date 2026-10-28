use files::FileId;
use indexing::TypeItemId;

use super::{DeriveStrategy, DerivedData};

pub fn check_derive_field_constraints(
    class_file: FileId,
    class_id: TypeItemId,
    derived: DerivedData,
) -> DeriveStrategy {
    DeriveStrategy::FieldConstraints {
        data_file: derived.data_file,
        data_id: derived.data_id,
        derived_type: derived.derived_type,
        class: (class_file, class_id),
    }
}
