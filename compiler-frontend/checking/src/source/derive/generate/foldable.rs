use std::sync::Arc;

use building_types::QueryResult;
use itertools::{Itertools, izip};
use smol_str::format_smolstr;

use crate::context::CheckContext;
use crate::core::substitute::RigidRenaming;
use crate::core::{ApplicationArgument, RowType, Type, TypeId, normalise, signature, toolkit};
use crate::source::derive::builder::DerivedTreeBuilder;
use crate::source::derive::field;
use crate::source::derive::variance::{
    ConstructorRecipe, RecordFieldRecipe, TraversalOperation, TraversalParameter, VarianceRecipe,
};
use crate::source::terms::{ElaboratedExpression, equations};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

use super::{
    DeriveHeadResult, DeriveStrategy, ResolvedMember, generated_member, resolve_known_member,
};

struct InstantiatedDataType {
    type_id: TypeId,
    constructor_arguments: Vec<ApplicationArgument>,
}

#[derive(Clone, Copy)]
pub(super) enum TraversalKind {
    Foldable,
    Bifoldable,
}

#[derive(Clone, Copy)]
enum FoldOperation {
    Right,
    Left,
    Map,
}

#[derive(Clone, Copy)]
enum Mappings<T> {
    Foldable(T),
    Bifoldable { first: T, second: T },
}

impl Mappings<ElaboratedExpression> {
    fn mapping_for(self, parameter: TraversalParameter) -> Option<ElaboratedExpression> {
        match (self, parameter) {
            (Mappings::Foldable(function), TraversalParameter::First) => Some(function),
            (Mappings::Bifoldable { first, .. }, TraversalParameter::First) => Some(first),
            (Mappings::Bifoldable { second, .. }, TraversalParameter::Second) => Some(second),
            (Mappings::Foldable(_), TraversalParameter::Second) => None,
        }
    }
}

struct DecodedFoldMember {
    member: ResolvedMember,
    renaming: Arc<RigidRenaming>,
    abstractions: Vec<signature::SkolemisedAbstraction>,
    implementation_type: TypeId,
    function_type: TypeId,
    mappings: Mappings<TypeId>,
    accumulator_type: TypeId,
    data_file: files::FileId,
    source: InstantiatedDataType,
    result_type: TypeId,
}

impl DecodedFoldMember {
    fn decode<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        member: ResolvedMember,
        data_file: files::FileId,
        traversal: TraversalKind,
        operation: FoldOperation,
    ) -> QueryResult<Option<DecodedFoldMember>>
    where
        Q: ExternalQueries,
    {
        // A directional fold supplies one transformation per traversed parameter, followed
        // by the initial accumulator and structural source. A monoidal fold omits the
        // explicit accumulator because its result type is the accumulator.
        //
        // List
        //
        //   foldr :: (a -> b -> b) -> b -> List a -> b
        //
        // Pair
        //
        //   bifoldr ::
        //     (a -> c -> c) -> (b -> c -> c) -> c -> Pair a b -> c
        //
        //   bifoldMap :: Monoid m => (a -> m) -> (b -> m) -> Pair a b -> m
        let argument_count = match (traversal, operation) {
            (TraversalKind::Foldable, FoldOperation::Right | FoldOperation::Left) => 3,
            (TraversalKind::Bifoldable, FoldOperation::Right | FoldOperation::Left) => 4,
            (TraversalKind::Foldable, FoldOperation::Map) => 2,
            (TraversalKind::Bifoldable, FoldOperation::Map) => 3,
        };
        let signature = signature::expect_term_signature(
            state,
            context,
            member.implementation_type,
            argument_count,
        )?;
        let arguments = signature.arguments().collect_vec();
        let signature::SkolemisedSignature { renaming, abstractions, result } = signature;

        let decoded = (traversal, operation, arguments.as_slice());
        let (source_type, accumulator_type, mappings) = match decoded {
            (
                TraversalKind::Foldable,
                FoldOperation::Right | FoldOperation::Left,
                [mapping, accumulator, source],
            ) => (*source, *accumulator, Mappings::Foldable(*mapping)),
            (
                TraversalKind::Bifoldable,
                FoldOperation::Right | FoldOperation::Left,
                [first, second, accumulator, source],
            ) => (*source, *accumulator, Mappings::Bifoldable { first: *first, second: *second }),
            (TraversalKind::Foldable, FoldOperation::Map, [mapping, source]) => {
                (*source, result, Mappings::Foldable(*mapping))
            }
            (TraversalKind::Bifoldable, FoldOperation::Map, [first, second, source]) => {
                (*source, result, Mappings::Bifoldable { first: *first, second: *second })
            }
            _ => return Ok(None),
        };

        let (_, source_arguments) = toolkit::extract_all_applications(state, context, source_type)?;
        let function_arguments = arguments.iter().copied();
        let function_type = context.intern_function_iter(function_arguments, result);

        Ok(Some(DecodedFoldMember {
            implementation_type: member.implementation_type,
            member,
            renaming,
            abstractions,
            function_type,
            mappings,
            accumulator_type,
            data_file,
            source: InstantiatedDataType {
                type_id: source_type,
                constructor_arguments: source_arguments,
            },
            result_type: result,
        }))
    }
}

pub(super) fn generate_fold_members<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    recipe: &VarianceRecipe,
    traversal: TraversalKind,
) -> QueryResult<Option<Vec<tree::InstanceMember>>>
where
    Q: ExternalQueries,
{
    let DeriveStrategy::VarianceConstraints { .. } = result.strategy else {
        return Ok(None);
    };
    let operations = match traversal {
        TraversalKind::Foldable => {
            (context.known_terms.foldr, context.known_terms.foldl, context.known_terms.fold_map)
        }
        TraversalKind::Bifoldable => (
            context.known_terms.bifoldr,
            context.known_terms.bifoldl,
            context.known_terms.bifold_map,
        ),
    };
    let (Some(foldr), Some(foldl), Some(fold_map)) = operations else {
        return Ok(None);
    };

    let Some(class) =
        toolkit::lookup_file_class(state, context, result.class_file, result.class_id)?
    else {
        return Ok(None);
    };

    // Foldable and Bifoldable dictionaries require both directions and their monoidal fold.
    // Match known identities rather than declaration order, and reject an incomplete
    // dictionary.
    let mut foldr_generated = false;
    let mut foldl_generated = false;
    let mut fold_map_generated = false;
    let mut members = Vec::with_capacity(class.members.len());
    for class_member in class.members.iter() {
        let resolution = (result.class_file, class_member.item_id);
        let member = if resolution == foldr {
            foldr_generated = true;
            generate_fold_member(
                state,
                context,
                result,
                instance_arguments,
                recipe,
                traversal,
                FoldOperation::Right,
            )?
        } else if resolution == foldl {
            foldl_generated = true;
            generate_fold_member(
                state,
                context,
                result,
                instance_arguments,
                recipe,
                traversal,
                FoldOperation::Left,
            )?
        } else if resolution == fold_map {
            fold_map_generated = true;
            generate_fold_member(
                state,
                context,
                result,
                instance_arguments,
                recipe,
                traversal,
                FoldOperation::Map,
            )?
        } else {
            return Ok(None);
        };
        let Some(member) = member else {
            return Ok(None);
        };
        members.push(member);
    }

    if !foldr_generated || !foldl_generated || !fold_map_generated {
        return Ok(None);
    }
    Ok(Some(members))
}

fn generate_fold_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    recipe: &VarianceRecipe,
    traversal: TraversalKind,
    operation: FoldOperation,
) -> QueryResult<Option<tree::InstanceMember>>
where
    Q: ExternalQueries,
{
    let DeriveStrategy::VarianceConstraints { data_file, .. } = result.strategy else {
        return Ok(None);
    };
    let resolution = match (traversal, operation) {
        (TraversalKind::Foldable, FoldOperation::Right) => context.known_terms.foldr,
        (TraversalKind::Foldable, FoldOperation::Left) => context.known_terms.foldl,
        (TraversalKind::Foldable, FoldOperation::Map) => context.known_terms.fold_map,
        (TraversalKind::Bifoldable, FoldOperation::Right) => context.known_terms.bifoldr,
        (TraversalKind::Bifoldable, FoldOperation::Left) => context.known_terms.bifoldl,
        (TraversalKind::Bifoldable, FoldOperation::Map) => context.known_terms.bifold_map,
    };
    let Some(resolution) = resolution else { return Ok(None) };

    state.with_implication(|state| {
        let Some(member) =
            resolve_known_member(state, context, result, instance_arguments, resolution)?
        else {
            return Ok(None);
        };
        let Some(member) =
            DecodedFoldMember::decode(state, context, member, data_file, traversal, operation)?
        else {
            return Ok(None);
        };

        let abstractions = equations::bind_signature_abstractions(state, &member.abstractions);

        let body = state.with_source_type_renaming(&member.renaming, |state| {
            emit_variance_fold(state, context, result.derive_id, &member, recipe, operation)
        })?;
        let Some(body) = body else { return Ok(None) };

        Ok(Some(generated_member(
            result.derive_id,
            (member.member.file_id, member.member.item_id),
            member.implementation_type,
            abstractions,
            body,
        )))
    })
}

fn emit_variance_fold<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derive_id: indexing::DeriveId,
    member: &DecodedFoldMember,
    recipe: &VarianceRecipe,
    operation: FoldOperation,
) -> QueryResult<Option<ElaboratedExpression>>
where
    Q: ExternalQueries,
{
    let mut builder = DerivedTreeBuilder::new(state, context, derive_id);

    let (mapping_binders, mapping_expressions) = match member.mappings {
        Mappings::Foldable(mapping) => {
            let function = builder.variable_binder("function", mapping);
            let expression = builder.variable(function);
            (vec![function], Mappings::Foldable(expression))
        }
        Mappings::Bifoldable { first, second } => {
            let first = builder.variable_binder("firstFunction", first);
            let second = builder.variable_binder("secondFunction", second);
            let expressions = Mappings::Bifoldable {
                first: builder.variable(first),
                second: builder.variable(second),
            };
            (vec![first, second], expressions)
        }
    };
    let (accumulator, initial) = match operation {
        FoldOperation::Right | FoldOperation::Left => {
            let accumulator = builder.variable_binder("accumulator", member.accumulator_type);
            (Some(accumulator), Some(builder.variable(accumulator)))
        }
        FoldOperation::Map => (None, None),
    };
    let value = builder.variable_binder("value", member.source.type_id);
    let value_expression = builder.variable(value);

    let mut alternatives = Vec::with_capacity(recipe.constructors.len());
    for constructor in &recipe.constructors {
        let Some(alternative) = emit_fold_alternative(
            &mut builder,
            member,
            constructor,
            mapping_expressions,
            operation,
            initial,
        )?
        else {
            return Ok(None);
        };
        alternatives.push(alternative);
    }

    let body = builder.case(member.result_type, vec![value_expression], alternatives);
    let mut binders = mapping_binders;
    binders.extend(accumulator);
    binders.push(value);
    Ok(Some(builder.lambda(member.function_type, binders, body)))
}

fn emit_fold_alternative<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    member: &DecodedFoldMember,
    constructor: &ConstructorRecipe,
    mapping_expressions: Mappings<ElaboratedExpression>,
    operation: FoldOperation,
    initial: Option<ElaboratedExpression>,
) -> QueryResult<Option<tree::CaseAlternative>>
where
    Q: ExternalQueries,
{
    let constructor_type = toolkit::lookup_file_term(
        builder.state,
        builder.context,
        member.data_file,
        constructor.constructor_id,
    )?;
    let source_fields = field::instantiate_constructor_fields(
        builder.state,
        builder.context,
        constructor_type,
        &member.source.constructor_arguments,
    )?;
    if source_fields.len() != constructor.fields.len() {
        return Ok(None);
    }

    let mut emitter = FoldEmitter {
        builder,
        mapping_expressions,
        operation,
        accumulator_type: member.accumulator_type,
    };
    let mut source_binders = Vec::with_capacity(source_fields.len());
    let mut active_fields = Vec::new();
    for (index, (source, operation)) in izip!(&source_fields, &constructor.fields).enumerate() {
        let source_binder =
            emitter.builder.variable_binder(&format_smolstr!("field{index}"), *source);
        let source_value = emitter.builder.variable(source_binder);
        source_binders.push(source_binder);
        if let Some(operation) = operation {
            active_fields.push((operation, source_value, *source));
        }
    }

    let pattern = emitter.builder.constructor_pattern(
        "constructor",
        member.source.type_id,
        (member.data_file, constructor.constructor_id),
        source_binders,
    );

    // A right fold nests source fields from right to left so the first field remains
    // the outermost application. A left fold threads the accumulator in source order.
    // FoldMap preserves that field order in a right-associated append tree.
    //
    //   data Product a = Product a Int a a
    //
    //   foldr function initial (Product first _ second third) =
    //     function first (function second (function third initial))
    //
    //   foldl function initial (Product first _ second third) =
    //     function (function (function initial first) second) third
    //
    //   foldMap function (Product first _ second third) =
    //     append (function first) (append (function second) (function third))
    let accumulated = match operation {
        FoldOperation::Right => {
            let Some(mut accumulated) = initial else {
                return Ok(None);
            };
            for &(operation, value, source_type) in active_fields.iter().rev() {
                let Some(folded) =
                    emitter.emit_operation(operation, value, Some(accumulated), source_type)?
                else {
                    return Ok(None);
                };
                accumulated = folded;
            }
            accumulated
        }
        FoldOperation::Left => {
            let Some(mut accumulated) = initial else {
                return Ok(None);
            };
            for &(operation, value, source_type) in &active_fields {
                let Some(folded) =
                    emitter.emit_operation(operation, value, Some(accumulated), source_type)?
                else {
                    return Ok(None);
                };
                accumulated = folded;
            }
            accumulated
        }
        FoldOperation::Map => {
            let mut contributions = Vec::with_capacity(active_fields.len());
            for &(operation, value, source_type) in &active_fields {
                let Some(contribution) =
                    emitter.emit_operation(operation, value, None, source_type)?
                else {
                    return Ok(None);
                };
                contributions.push(contribution);
            }
            let Some(combined) = emitter.combine_contributions(contributions)? else {
                return Ok(None);
            };
            combined
        }
    };
    let body = emitter.builder.subtype(accumulated, member.result_type)?;
    Ok(Some(emitter.builder.alternative(vec![pattern], body)))
}

struct FoldEmitter<'builder, 'state, 'context, 'queries, Q: ExternalQueries> {
    builder: &'builder mut DerivedTreeBuilder<'state, 'context, 'queries, Q>,
    mapping_expressions: Mappings<ElaboratedExpression>,
    operation: FoldOperation,
    accumulator_type: TypeId,
}

impl<Q> FoldEmitter<'_, '_, '_, '_, Q>
where
    Q: ExternalQueries,
{
    fn emit_operation(
        &mut self,
        operation: &TraversalOperation,
        value: ElaboratedExpression,
        accumulator: Option<ElaboratedExpression>,
        source_type: TypeId,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        match operation {
            TraversalOperation::Parameter { parameter } => {
                let Some(mapping_expression) = self.mapping_expressions.mapping_for(*parameter)
                else {
                    return Ok(None);
                };
                let applied = match self.operation {
                    FoldOperation::Right => {
                        let Some(accumulator) = accumulator else {
                            return Ok(None);
                        };
                        let Some(applied) = self.builder.apply(mapping_expression, value)? else {
                            return Ok(None);
                        };
                        let Some(applied) = self.builder.apply(applied, accumulator)? else {
                            return Ok(None);
                        };
                        applied
                    }
                    FoldOperation::Left => {
                        let Some(accumulator) = accumulator else {
                            return Ok(None);
                        };
                        let Some(applied) = self.builder.apply(mapping_expression, accumulator)?
                        else {
                            return Ok(None);
                        };
                        let Some(applied) = self.builder.apply(applied, value)? else {
                            return Ok(None);
                        };
                        applied
                    }
                    FoldOperation::Map => {
                        let Some(applied) = self.builder.apply(mapping_expression, value)? else {
                            return Ok(None);
                        };
                        applied
                    }
                };
                Ok(Some(self.builder.subtype(applied, self.accumulator_type)?))
            }
            TraversalOperation::UnaryApplication { argument, .. } => {
                let Some((_, source_argument)) = toolkit::decompose_type_application(
                    self.builder.state,
                    self.builder.context,
                    source_type,
                )?
                else {
                    return Ok(None);
                };

                // Delegate the outer constructor to its Foldable instance. The generated
                // transformer folds every contained value according to the argument recipe.
                //
                //   newtype Compose f g a = Compose (f (g a))
                //
                //   foldr function initial (Compose value) =
                //     foldr
                //       (\element accumulator -> foldr function accumulator element)
                //       initial
                //       value
                let Some(transformer) = self.emit_transformer(argument, source_argument)? else {
                    return Ok(None);
                };
                let operation = match self.operation {
                    FoldOperation::Right => self.builder.context.known_terms.foldr,
                    FoldOperation::Left => self.builder.context.known_terms.foldl,
                    FoldOperation::Map => self.builder.context.known_terms.fold_map,
                };
                let Some(operation) = operation else {
                    return Ok(None);
                };
                let operation = self.builder.term_reference(operation)?;
                let Some(operation) = self.builder.apply(operation, transformer)? else {
                    return Ok(None);
                };
                let operation = match self.operation {
                    FoldOperation::Right | FoldOperation::Left => {
                        let Some(accumulator) = accumulator else {
                            return Ok(None);
                        };
                        let Some(operation) = self.builder.apply(operation, accumulator)? else {
                            return Ok(None);
                        };
                        operation
                    }
                    FoldOperation::Map => operation,
                };
                let Some(folded) = self.builder.apply(operation, value)? else {
                    return Ok(None);
                };
                Ok(Some(self.builder.subtype(folded, self.accumulator_type)?))
            }
            TraversalOperation::BinaryApplication { arguments, .. } => {
                let (first, second) = arguments.operations();
                self.emit_binary_fold(first, second, value, accumulator, source_type)
            }
            TraversalOperation::Record { fields } => {
                self.emit_record_fold(fields, value, accumulator, source_type)
            }
            // Function operations are rejected during variance analysis.
            TraversalOperation::Function { .. } => Ok(None),
        }
    }

    fn emit_transformer(
        &mut self,
        operation: &TraversalOperation,
        source_type: TypeId,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let element = self.builder.variable_binder("element", source_type);
        let element_value = self.builder.variable(element);
        let (arguments, function_arguments, body) = match self.operation {
            FoldOperation::Right => {
                let accumulator =
                    self.builder.variable_binder("accumulator", self.accumulator_type);
                let accumulator_value = self.builder.variable(accumulator);
                let Some(body) = self.emit_operation(
                    operation,
                    element_value,
                    Some(accumulator_value),
                    source_type,
                )?
                else {
                    return Ok(None);
                };
                (vec![element, accumulator], vec![source_type, self.accumulator_type], body)
            }
            FoldOperation::Left => {
                let accumulator =
                    self.builder.variable_binder("accumulator", self.accumulator_type);
                let accumulator_value = self.builder.variable(accumulator);
                let Some(body) = self.emit_operation(
                    operation,
                    element_value,
                    Some(accumulator_value),
                    source_type,
                )?
                else {
                    return Ok(None);
                };
                (vec![accumulator, element], vec![self.accumulator_type, source_type], body)
            }
            FoldOperation::Map => {
                let Some(body) =
                    self.emit_operation(operation, element_value, None, source_type)?
                else {
                    return Ok(None);
                };
                (vec![element], vec![source_type], body)
            }
        };
        let function_type =
            self.builder.context.intern_function_iter(function_arguments, self.accumulator_type);
        Ok(Some(self.builder.lambda(function_type, arguments, body)))
    }

    fn emit_binary_fold(
        &mut self,
        first: Option<&TraversalOperation>,
        second: Option<&TraversalOperation>,
        value: ElaboratedExpression,
        accumulator: Option<ElaboratedExpression>,
        source_type: TypeId,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let Some((source_function, source_second)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            source_type,
        )?
        else {
            return Ok(None);
        };
        let Some((_, source_first)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            source_function,
        )?
        else {
            return Ok(None);
        };

        let first_transformer = if let Some(first) = first {
            let Some(transformer) = self.emit_transformer(first, source_first)? else {
                return Ok(None);
            };
            transformer
        } else {
            let Some(transformer) = self.emit_ignoring_transformer(source_first)? else {
                return Ok(None);
            };
            transformer
        };
        let second_transformer = match second {
            Some(second) => {
                let Some(transformer) = self.emit_transformer(second, source_second)? else {
                    return Ok(None);
                };
                transformer
            }
            None => {
                let Some(transformer) = self.emit_ignoring_transformer(source_second)? else {
                    return Ok(None);
                };
                transformer
            }
        };

        // Bifoldable still requires a transformer for an inactive argument. A directional
        // fold returns the accumulator unchanged; foldMap returns mempty.
        //
        //   newtype LeftDuplicate p a = LeftDuplicate (p a Int)
        //
        //   foldr function initial (LeftDuplicate value) =
        //     bifoldr function (\_ accumulator -> accumulator) initial value
        //
        //   foldMap function (LeftDuplicate value) =
        //     bifoldMap function (\_ -> mempty) value
        let operation = match self.operation {
            FoldOperation::Right => self.builder.context.known_terms.bifoldr,
            FoldOperation::Left => self.builder.context.known_terms.bifoldl,
            FoldOperation::Map => self.builder.context.known_terms.bifold_map,
        };
        let Some(operation) = operation else {
            return Ok(None);
        };
        let operation = self.builder.term_reference(operation)?;
        let Some(operation) = self.builder.apply(operation, first_transformer)? else {
            return Ok(None);
        };
        let Some(operation) = self.builder.apply(operation, second_transformer)? else {
            return Ok(None);
        };
        let operation = match self.operation {
            FoldOperation::Right | FoldOperation::Left => {
                let Some(accumulator) = accumulator else {
                    return Ok(None);
                };
                let Some(operation) = self.builder.apply(operation, accumulator)? else {
                    return Ok(None);
                };
                operation
            }
            FoldOperation::Map => operation,
        };
        let Some(folded) = self.builder.apply(operation, value)? else {
            return Ok(None);
        };
        Ok(Some(self.builder.subtype(folded, self.accumulator_type)?))
    }

    fn emit_ignoring_transformer(
        &mut self,
        source_type: TypeId,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let element = self.builder.variable_binder("ignored", source_type);
        let (arguments, function_arguments, body) = match self.operation {
            FoldOperation::Right => {
                let accumulator =
                    self.builder.variable_binder("accumulator", self.accumulator_type);
                (
                    vec![element, accumulator],
                    vec![source_type, self.accumulator_type],
                    self.builder.variable(accumulator),
                )
            }
            FoldOperation::Left => {
                let accumulator =
                    self.builder.variable_binder("accumulator", self.accumulator_type);
                (
                    vec![accumulator, element],
                    vec![self.accumulator_type, source_type],
                    self.builder.variable(accumulator),
                )
            }
            FoldOperation::Map => {
                let Some(mempty) = self.emit_mempty()? else {
                    return Ok(None);
                };
                (vec![element], vec![source_type], mempty)
            }
        };
        let function_type =
            self.builder.context.intern_function_iter(function_arguments, self.accumulator_type);
        Ok(Some(self.builder.lambda(function_type, arguments, body)))
    }

    fn emit_record_fold(
        &mut self,
        fields: &[RecordFieldRecipe],
        value: ElaboratedExpression,
        accumulator: Option<ElaboratedExpression>,
        source_type: TypeId,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let Some(source_row) =
            extract_record_row(self.builder.state, self.builder.context, source_type)?
        else {
            return Ok(None);
        };

        // Record recipes follow canonical row order. Fold right reverses that sequence
        // while constructing its nested applications; fold left consumes it directly.
        let mut active_fields = Vec::with_capacity(fields.len());
        for field in fields {
            let Some(source_field) = source_row.fields.iter().find(|row| row.label == field.label)
            else {
                return Ok(None);
            };
            active_fields.push((field, source_field.id));
        }

        let folded = match self.operation {
            FoldOperation::Right => {
                let Some(mut accumulated) = accumulator else {
                    return Ok(None);
                };
                for &(field, source_field) in active_fields.iter().rev() {
                    let accessed =
                        self.builder.record_access(value, field.label.clone(), source_field);
                    let Some(folded) = self.emit_operation(
                        &field.operation,
                        accessed,
                        Some(accumulated),
                        source_field,
                    )?
                    else {
                        return Ok(None);
                    };
                    accumulated = folded;
                }
                accumulated
            }
            FoldOperation::Left => {
                let Some(mut accumulated) = accumulator else {
                    return Ok(None);
                };
                for &(field, source_field) in &active_fields {
                    let accessed =
                        self.builder.record_access(value, field.label.clone(), source_field);
                    let Some(folded) = self.emit_operation(
                        &field.operation,
                        accessed,
                        Some(accumulated),
                        source_field,
                    )?
                    else {
                        return Ok(None);
                    };
                    accumulated = folded;
                }
                accumulated
            }
            FoldOperation::Map => {
                let mut contributions = Vec::with_capacity(active_fields.len());
                for &(field, source_field) in &active_fields {
                    let accessed =
                        self.builder.record_access(value, field.label.clone(), source_field);
                    let Some(contribution) =
                        self.emit_operation(&field.operation, accessed, None, source_field)?
                    else {
                        return Ok(None);
                    };
                    contributions.push(contribution);
                }
                let Some(combined) = self.combine_contributions(contributions)? else {
                    return Ok(None);
                };
                combined
            }
        };
        Ok(Some(folded))
    }

    fn combine_contributions(
        &mut self,
        contributions: Vec<ElaboratedExpression>,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // FoldMap combines active contributions in logical field order. Construct the
        // right-associated append tree from the final contribution, using mempty only
        // when the constructor or record contains no active fields.
        let contributions = contributions.into_iter().rev();
        let mut contributions = contributions;
        let Some(mut accumulated) = contributions.next() else {
            return self.emit_mempty();
        };

        for contribution in contributions {
            let Some(append) = self.builder.context.known_terms.append else {
                return Ok(None);
            };
            let append = self.builder.term_reference(append)?;
            let Some(append) = self.builder.apply(append, contribution)? else {
                return Ok(None);
            };
            let Some(appended) = self.builder.apply(append, accumulated)? else {
                return Ok(None);
            };
            accumulated = self.builder.subtype(appended, self.accumulator_type)?;
        }
        Ok(Some(accumulated))
    }

    fn emit_mempty(&mut self) -> QueryResult<Option<ElaboratedExpression>> {
        let Some(mempty) = self.builder.context.known_terms.mempty else {
            return Ok(None);
        };
        let mempty = self.builder.term_reference(mempty)?;
        Ok(Some(self.builder.subtype(mempty, self.accumulator_type)?))
    }
}

fn extract_record_row<'q, Q>(
    state: &mut CheckState,
    context: &CheckContext<'q, Q>,
    type_id: TypeId,
) -> QueryResult<Option<&'q RowType>>
where
    Q: ExternalQueries,
{
    let Some((_, row)) = toolkit::decompose_type_application(state, context, type_id)? else {
        return Ok(None);
    };
    let row = normalise::expand(state, context, row)?;
    let Type::Row(row) = *context.lookup_type(row) else {
        return Ok(None);
    };
    Ok(Some(context.lookup_row_type(row)))
}
