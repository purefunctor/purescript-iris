pub mod head;
pub mod member;

pub mod builder;
pub mod contravariant;
pub mod eq1_ord1;
pub mod eq_ord;
pub mod field;
pub mod foldable;
pub mod functor;
pub mod generate;
pub mod generic;
pub mod newtype;
pub mod tools;
pub mod traversable;
pub mod variance;

use building_types::QueryResult;
use files::FileId;
use indexing::{DeriveItemId, TypeItemId};

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::TypeId;
use crate::source::derive::variance::VarianceConfig;
use crate::state::CheckState;

#[derive(Clone, Copy)]
enum DeriveDispatch {
    Eq,
    Eq1,
    Functor,
    Bifunctor,
    Contravariant,
    Profunctor,
    Foldable,
    Bifoldable,
    Traversable,
    Bitraversable,
    Newtype,
    Generic,
    Ord,
    Ord1,
    Unsupported,
}

impl DeriveDispatch {
    fn arity(self) -> Option<usize> {
        match self {
            DeriveDispatch::Newtype | DeriveDispatch::Generic => Some(2),
            DeriveDispatch::Unsupported => None,
            _ => Some(1),
        }
    }

    fn accepts_foreign_data(self) -> bool {
        matches!(self, DeriveDispatch::Eq1 | DeriveDispatch::Ord1)
    }
}

#[derive(Clone, Copy)]
pub(super) struct DerivedData {
    derived_type: TypeId,
    data_file: FileId,
    data_id: TypeItemId,
}

#[derive(Clone, Copy)]
pub(super) enum DeriveStrategy {
    FieldConstraints {
        data_file: FileId,
        data_id: TypeItemId,
        derived_type: TypeId,
        class: (FileId, TypeItemId),
    },
    DelegateConstraint {
        derived_type: TypeId,
        class: (FileId, TypeItemId),
    },
    NewtypeDeriveConstraint {
        delegate_constraint: TypeId,
    },
    NewtypeClass,
    Generic {
        data_file: FileId,
        data_id: TypeItemId,
    },
    VarianceConstraints {
        data_file: FileId,
        data_id: TypeItemId,
        derived_type: TypeId,
        config: VarianceConfig,
    },
}

pub struct DeriveHeadResult {
    derive_id: indexing::DeriveId,
    item_id: DeriveItemId,
    signature: TypeId,
    constraints: Vec<TypeId>,
    class_file: FileId,
    class_id: TypeItemId,
    arguments: Vec<TypeId>,
    strategy: DeriveStrategy,
}

fn derive_dispatch<Q>(
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
) -> DeriveDispatch
where
    Q: ExternalQueries,
{
    let class = Some((class_file, class_id));
    if class == context.known_types.eq {
        DeriveDispatch::Eq
    } else if class == context.known_types.eq1 {
        DeriveDispatch::Eq1
    } else if class == context.known_types.functor {
        DeriveDispatch::Functor
    } else if class == context.known_types.bifunctor {
        DeriveDispatch::Bifunctor
    } else if class == context.known_types.contravariant {
        DeriveDispatch::Contravariant
    } else if class == context.known_types.profunctor {
        DeriveDispatch::Profunctor
    } else if class == context.known_types.foldable {
        DeriveDispatch::Foldable
    } else if class == context.known_types.bifoldable {
        DeriveDispatch::Bifoldable
    } else if class == context.known_types.traversable {
        DeriveDispatch::Traversable
    } else if class == context.known_types.bitraversable {
        DeriveDispatch::Bitraversable
    } else if class == context.known_types.newtype {
        DeriveDispatch::Newtype
    } else if class == context.known_types.generic {
        DeriveDispatch::Generic
    } else if class == context.known_types.ord {
        DeriveDispatch::Ord
    } else if class == context.known_types.ord1 {
        DeriveDispatch::Ord1
    } else {
        DeriveDispatch::Unsupported
    }
}

pub fn check_derive_declarations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<Vec<DeriveHeadResult>>
where
    Q: ExternalQueries,
{
    head::check_derive_declarations(state, context)
}

pub fn check_derive_members<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derives: &[DeriveHeadResult],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    member::check_derive_members(state, context, derives)
}
