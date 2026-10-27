use files::FileId;
use indexing::TypeItemId;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::error::ErrorKind;
use crate::state::CheckState;

use super::{DeriveStrategy, DerivedData};

pub fn check_derive_eq1<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    derived: DerivedData,
) -> Option<DeriveStrategy>
where
    Q: ExternalQueries,
{
    let Some(eq) = context.known_types.eq else {
        state.insert_error(ErrorKind::CannotDeriveClass { class_file, class_id });
        return None;
    };

    Some(DeriveStrategy::DelegateConstraint { derived_type: derived.derived_type, class: eq })
}

pub fn check_derive_ord1<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    derived: DerivedData,
) -> Option<DeriveStrategy>
where
    Q: ExternalQueries,
{
    let Some(ord) = context.known_types.ord else {
        state.insert_error(ErrorKind::CannotDeriveClass { class_file, class_id });
        return None;
    };

    Some(DeriveStrategy::DelegateConstraint { derived_type: derived.derived_type, class: ord })
}
