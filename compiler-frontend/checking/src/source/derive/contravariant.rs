use files::FileId;
use indexing::TypeItemId;

use crate::ExternalQueries;
use crate::context::CheckContext;

use super::variance::{FunctionPolicy, ParameterConfig, Variance, VarianceConfig};
use super::{DeriveStrategy, DerivedData};

pub fn check_derive_contravariant(
    class_file: FileId,
    class_id: TypeItemId,
    derived: DerivedData,
) -> DeriveStrategy {
    let parameter = ParameterConfig {
        variance: Variance::Contravariant,
        unary_class: Some((class_file, class_id)),
        function_policy: FunctionPolicy::Allow,
    };
    let config = VarianceConfig::Single { parameter, binary_class: None };

    DeriveStrategy::VarianceConstraints {
        data_file: derived.data_file,
        data_id: derived.data_id,
        derived_type: derived.derived_type,
        config,
    }
}

pub fn check_derive_profunctor<Q>(context: &CheckContext<Q>, derived: DerivedData) -> DeriveStrategy
where
    Q: ExternalQueries,
{
    let config = VarianceConfig::Pair {
        first: ParameterConfig {
            variance: Variance::Contravariant,
            unary_class: context.known_types.contravariant,
            function_policy: FunctionPolicy::Allow,
        },
        second: ParameterConfig {
            variance: Variance::Covariant,
            unary_class: context.known_types.functor,
            function_policy: FunctionPolicy::Allow,
        },
        binary_class: None,
    };

    DeriveStrategy::VarianceConstraints {
        data_file: derived.data_file,
        data_id: derived.data_id,
        derived_type: derived.derived_type,
        config,
    }
}
