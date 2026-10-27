use files::FileId;
use indexing::TypeItemId;

use crate::ExternalQueries;
use crate::context::CheckContext;

use super::variance::{FunctionPolicy, ParameterConfig, Variance, VarianceConfig};
use super::{DeriveStrategy, DerivedData};

pub fn check_derive_foldable<Q>(
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    derived: DerivedData,
) -> DeriveStrategy
where
    Q: ExternalQueries,
{
    let parameter = ParameterConfig {
        variance: Variance::Covariant,
        unary_class: Some((class_file, class_id)),
        function_policy: FunctionPolicy::Reject,
    };
    let config = VarianceConfig::Single { parameter, binary_class: context.known_types.bifoldable };

    DeriveStrategy::VarianceConstraints {
        data_file: derived.data_file,
        data_id: derived.data_id,
        derived_type: derived.derived_type,
        config,
    }
}

pub fn check_derive_bifoldable<Q>(
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    derived: DerivedData,
) -> DeriveStrategy
where
    Q: ExternalQueries,
{
    let parameter = ParameterConfig {
        variance: Variance::Covariant,
        unary_class: context.known_types.foldable,
        function_policy: FunctionPolicy::Reject,
    };
    let config = VarianceConfig::Pair {
        first: parameter,
        second: parameter,
        binary_class: Some((class_file, class_id)),
    };

    DeriveStrategy::VarianceConstraints {
        data_file: derived.data_file,
        data_id: derived.data_id,
        derived_type: derived.derived_type,
        config,
    }
}
