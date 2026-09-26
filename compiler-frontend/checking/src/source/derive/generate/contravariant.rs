use std::rc::Rc;

use building_types::QueryResult;
use itertools::{Itertools, izip};
use smol_str::format_smolstr;

use crate::context::CheckContext;
use crate::core::substitute::RigidRenaming;
use crate::core::{ApplicationArgument, RowType, Type, TypeId, normalise, signature, toolkit};
use crate::source::derive::builder::DerivedTreeBuilder;
use crate::source::derive::field;
use crate::source::derive::variance::{
    ConstructorRecipe, RecordFieldRecipe, TraversalOperation, TraversalParameter, Variance,
    VarianceRecipe,
};
use crate::source::terms::{ElaboratedExpression, equations};
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

use super::{DeriveHeadResult, DeriveStrategy, ResolvedMember, generated_member, resolve_member};

struct InstantiatedDataType {
    type_id: TypeId,
    constructor_arguments: Vec<ApplicationArgument>,
}

#[derive(Clone, Copy)]
pub(super) enum TraversalKind {
    Contravariant,
    Profunctor,
}

#[derive(Clone, Copy)]
enum Mappings<T> {
    Contravariant(T),
    Profunctor { first: T, second: T },
}

impl Mappings<ElaboratedExpression> {
    fn mapping_for(self, parameter: TraversalParameter) -> Option<ElaboratedExpression> {
        match (self, parameter) {
            (Mappings::Contravariant(function), TraversalParameter::First) => Some(function),
            (Mappings::Profunctor { first, .. }, TraversalParameter::First) => Some(first),
            (Mappings::Profunctor { second, .. }, TraversalParameter::Second) => Some(second),
            (Mappings::Contravariant(_), TraversalParameter::Second) => None,
        }
    }
}

struct DecodedTraversalMember {
    member: ResolvedMember,
    renaming: Rc<RigidRenaming>,
    abstractions: Vec<signature::SkolemisedAbstraction>,
    implementation_type: TypeId,
    function_type: TypeId,
    mappings: Mappings<TypeId>,
    data_file: files::FileId,
    source: InstantiatedDataType,
    target: InstantiatedDataType,
}

impl DecodedTraversalMember {
    fn decode<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        member: ResolvedMember,
        data_file: files::FileId,
        traversal: TraversalKind,
    ) -> QueryResult<Option<DecodedTraversalMember>>
    where
        Q: ExternalQueries,
    {
        let argument_count = match traversal {
            TraversalKind::Contravariant => 2,
            TraversalKind::Profunctor => 3,
        };
        let signature = signature::expect_term_signature(
            state,
            context,
            member.implementation_type,
            argument_count,
        )?;
        let arguments = signature.arguments().collect_vec();
        let signature::SkolemisedSignature { renaming, abstractions, result } = signature;

        let (source_type, mappings) = match (traversal, arguments.as_slice()) {
            (TraversalKind::Contravariant, [mapping, source]) => {
                (*source, Mappings::Contravariant(*mapping))
            }
            (TraversalKind::Profunctor, [first, second, source]) => {
                (*source, Mappings::Profunctor { first: *first, second: *second })
            }
            _ => return Ok(None),
        };

        let (_, source_arguments) = toolkit::extract_all_applications(state, context, source_type)?;
        let (_, target_arguments) = toolkit::extract_all_applications(state, context, result)?;
        let function_arguments = arguments.iter().copied();
        let function_type = context.intern_function_iter(function_arguments, result);

        Ok(Some(DecodedTraversalMember {
            implementation_type: member.implementation_type,
            member,
            renaming,
            abstractions,
            function_type,
            mappings,
            data_file,
            source: InstantiatedDataType {
                type_id: source_type,
                constructor_arguments: source_arguments,
            },
            target: InstantiatedDataType {
                type_id: result,
                constructor_arguments: target_arguments,
            },
        }))
    }
}

pub(super) fn generate_traversal_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    recipe: &VarianceRecipe,
    traversal: TraversalKind,
) -> QueryResult<Option<tree::InstanceMember>>
where
    Q: ExternalQueries,
{
    state.with_implication(|state| {
        let DeriveStrategy::VarianceConstraints { data_file, .. } = result.strategy else {
            return Ok(None);
        };

        let Some(member) = resolve_member(state, context, result, instance_arguments)? else {
            return Ok(None);
        };
        let Some(member) =
            DecodedTraversalMember::decode(state, context, member, data_file, traversal)?
        else {
            return Ok(None);
        };

        let abstractions = equations::bind_signature_abstractions(state, &member.abstractions);

        let body = state.with_source_type_renaming(&member.renaming, |state| {
            emit_variance_traversal(state, context, result.derive_id, &member, recipe)
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

fn emit_variance_traversal<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derive_id: indexing::DeriveId,
    member: &DecodedTraversalMember,
    recipe: &VarianceRecipe,
) -> QueryResult<Option<ElaboratedExpression>>
where
    Q: ExternalQueries,
{
    let mut builder = DerivedTreeBuilder::new(state, context, derive_id);
    let (mapping_binders, mapping_expressions) = match member.mappings {
        Mappings::Contravariant(mapping) => {
            let function = builder.variable_binder("function", mapping);
            let expression = builder.variable(function);
            (vec![function], Mappings::Contravariant(expression))
        }
        Mappings::Profunctor { first, second } => {
            let first = builder.variable_binder("firstFunction", first);
            let second = builder.variable_binder("secondFunction", second);
            let expressions = Mappings::Profunctor {
                first: builder.variable(first),
                second: builder.variable(second),
            };
            (vec![first, second], expressions)
        }
    };
    let value = builder.variable_binder("value", member.source.type_id);
    let value_expression = builder.variable(value);

    let mut alternatives = Vec::with_capacity(recipe.constructors.len());
    for constructor in &recipe.constructors {
        let Some(alternative) =
            emit_traversal_alternative(&mut builder, member, constructor, mapping_expressions)?
        else {
            return Ok(None);
        };
        alternatives.push(alternative);
    }

    let body = builder.case(member.target.type_id, vec![value_expression], alternatives);
    let mut binders = mapping_binders;
    binders.push(value);
    Ok(Some(builder.lambda(member.function_type, binders, body)))
}

fn emit_traversal_alternative<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    member: &DecodedTraversalMember,
    constructor: &ConstructorRecipe,
    mapping_expressions: Mappings<ElaboratedExpression>,
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
    let target_fields = field::instantiate_constructor_fields(
        builder.state,
        builder.context,
        constructor_type,
        &member.target.constructor_arguments,
    )?;

    if source_fields.len() != target_fields.len() || source_fields.len() != constructor.fields.len()
    {
        return Ok(None);
    }

    let mut emitter = ContravariantTraversalEmitter { builder, mapping_expressions };
    let mut binders = Vec::with_capacity(source_fields.len());
    let mut values = Vec::with_capacity(source_fields.len());
    for (index, (source, target, operation)) in
        izip!(&source_fields, &target_fields, &constructor.fields).enumerate()
    {
        let binder = emitter.builder.variable_binder(&format_smolstr!("field{index}"), *source);
        let value = emitter.builder.variable(binder);
        let value = if let Some(operation) = operation {
            let traversal =
                TraversalContext { source_type: *source, target_type: *target, function_depth: 0 };
            let Some(value) = emitter.emit_traversal(operation, value, traversal)? else {
                return Ok(None);
            };
            value
        } else {
            value
        };
        binders.push(binder);
        values.push(value);
    }

    let pattern = emitter.builder.constructor_pattern(
        "constructor",
        member.source.type_id,
        (member.data_file, constructor.constructor_id),
        binders,
    );
    let mut reconstructed =
        emitter.builder.term_reference((member.data_file, constructor.constructor_id))?;
    for value in values {
        let Some(applied) = emitter.builder.apply(reconstructed, value)? else {
            return Ok(None);
        };
        reconstructed = applied;
    }
    let reconstructed = emitter.builder.subtype(reconstructed, member.target.type_id)?;
    Ok(Some(emitter.builder.alternative(vec![pattern], reconstructed)))
}

#[derive(Clone, Copy)]
struct TraversalContext {
    source_type: TypeId,
    target_type: TypeId,
    function_depth: usize,
}

struct ContravariantTraversalEmitter<'builder, 'state, 'context, 'queries, Q: ExternalQueries> {
    builder: &'builder mut DerivedTreeBuilder<'state, 'context, 'queries, Q>,
    mapping_expressions: Mappings<ElaboratedExpression>,
}

impl<Q> ContravariantTraversalEmitter<'_, '_, '_, '_, Q>
where
    Q: ExternalQueries,
{
    fn emit_traversal(
        &mut self,
        operation: &TraversalOperation,
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        match operation {
            TraversalOperation::Parameter { parameter } => {
                let Some(mapping_expression) = self.mapping_expressions.mapping_for(*parameter)
                else {
                    return Ok(None);
                };
                let Some(mapped) = self.builder.apply(mapping_expression, value)? else {
                    return Ok(None);
                };
                Ok(Some(self.builder.subtype(mapped, traversal.target_type)?))
            }
            TraversalOperation::Function { argument, result } => {
                self.emit_function(argument.as_deref(), result.as_deref(), value, traversal)
            }
            TraversalOperation::UnaryApplication { argument_variance, argument } => {
                self.emit_unary(*argument_variance, argument, value, traversal)
            }
            TraversalOperation::BinaryApplication { first_variance, arguments } => {
                let (first, second) = arguments.operations();
                self.emit_binary(*first_variance, first, second, value, traversal)
            }
            TraversalOperation::Record { fields } => {
                self.emit_record_traversal(fields, value, traversal)
            }
        }
    }

    fn emit_unary(
        &mut self,
        argument_variance: Variance,
        argument: &TraversalOperation,
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let Some((_, source_argument)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            traversal.source_type,
        )?
        else {
            return Ok(None);
        };
        let Some((_, target_argument)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            traversal.target_type,
        )?
        else {
            return Ok(None);
        };

        // A covariant edge transforms the source argument into the target argument. A
        // contravariant edge reverses that obligation because `cmap` consumes the target-to-
        // source transformation.
        let argument_context = match argument_variance {
            Variance::Covariant => TraversalContext {
                source_type: source_argument,
                target_type: target_argument,
                function_depth: traversal.function_depth,
            },
            Variance::Contravariant => TraversalContext {
                source_type: target_argument,
                target_type: source_argument,
                function_depth: traversal.function_depth,
            },
        };
        let Some(transformer) = self.emit_transformer(argument, argument_context)? else {
            return Ok(None);
        };

        let operation = match argument_variance {
            Variance::Covariant => self.builder.context.known_terms.map,
            Variance::Contravariant => self.builder.context.known_terms.cmap,
        };
        let Some(operation) = operation else {
            return Ok(None);
        };
        let operation = self.builder.term_reference(operation)?;
        let Some(operation) = self.builder.apply(operation, transformer)? else {
            return Ok(None);
        };
        let Some(mapped) = self.builder.apply(operation, value)? else {
            return Ok(None);
        };
        Ok(Some(self.builder.subtype(mapped, traversal.target_type)?))
    }

    fn emit_function(
        &mut self,
        argument: Option<&TraversalOperation>,
        result: Option<&TraversalOperation>,
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // The target function's argument must be transformed into the source argument before
        // applying `value`; its source result is then transformed into the target result.
        let Some((source_argument, source_result)) = toolkit::decompose_function(
            self.builder.state,
            self.builder.context,
            traversal.source_type,
        )?
        else {
            return Ok(None);
        };
        let Some((target_argument, target_result)) = toolkit::decompose_function(
            self.builder.state,
            self.builder.context,
            traversal.target_type,
        )?
        else {
            return Ok(None);
        };

        let input_name = match traversal.function_depth {
            0 => "argument".into(),
            depth => format_smolstr!("argument{depth}"),
        };
        let input = self.builder.variable_binder(&input_name, target_argument);
        let mut input_value = self.builder.variable(input);
        if let Some(operation) = argument {
            let argument_context = TraversalContext {
                source_type: target_argument,
                target_type: source_argument,
                function_depth: traversal.function_depth + 1,
            };
            let Some(transformed) =
                self.emit_traversal(operation, input_value, argument_context)?
            else {
                return Ok(None);
            };
            input_value = transformed;
        }

        let Some(output) = self.builder.apply(value, input_value)? else {
            return Ok(None);
        };
        let output = if let Some(operation) = result {
            let result_context = TraversalContext {
                source_type: source_result,
                target_type: target_result,
                function_depth: traversal.function_depth + 1,
            };
            let Some(output) = self.emit_traversal(operation, output, result_context)? else {
                return Ok(None);
            };
            output
        } else {
            output
        };
        let output = self.builder.subtype(output, target_result)?;
        Ok(Some(self.builder.lambda(traversal.target_type, vec![input], output)))
    }

    fn emit_binary(
        &mut self,
        first_variance: Variance,
        first: Option<&TraversalOperation>,
        second: Option<&TraversalOperation>,
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let Some((source_function, source_second)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            traversal.source_type,
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
        let Some((target_function, target_second)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            traversal.target_type,
        )?
        else {
            return Ok(None);
        };
        let Some((_, target_first)) = toolkit::decompose_type_application(
            self.builder.state,
            self.builder.context,
            target_function,
        )?
        else {
            return Ok(None);
        };

        // Bifunctor transforms both arguments covariantly. Profunctor reverses only the
        // first obligation, matching `dimap :: (a -> b) -> (c -> d) -> p b c -> p a d`.
        let first_context = match first_variance {
            Variance::Covariant => TraversalContext {
                source_type: source_first,
                target_type: target_first,
                function_depth: traversal.function_depth,
            },
            Variance::Contravariant => TraversalContext {
                source_type: target_first,
                target_type: source_first,
                function_depth: traversal.function_depth,
            },
        };
        let first_transformer = if let Some(first) = first {
            let Some(transformer) = self.emit_transformer(first, first_context)? else {
                return Ok(None);
            };
            transformer
        } else {
            self.emit_identity(first_context)?
        };

        let second_context = TraversalContext {
            source_type: source_second,
            target_type: target_second,
            function_depth: traversal.function_depth,
        };
        let second_transformer = if let Some(second) = second {
            let Some(transformer) = self.emit_transformer(second, second_context)? else {
                return Ok(None);
            };
            transformer
        } else {
            self.emit_identity(second_context)?
        };

        let operation = match first_variance {
            Variance::Covariant => self.builder.context.known_terms.bimap,
            Variance::Contravariant => self.builder.context.known_terms.dimap,
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
        let Some(mapped) = self.builder.apply(operation, value)? else {
            return Ok(None);
        };
        Ok(Some(self.builder.subtype(mapped, traversal.target_type)?))
    }

    fn emit_transformer(
        &mut self,
        operation: &TraversalOperation,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let input = self.builder.variable_binder("element", traversal.source_type);
        let value = self.builder.variable(input);
        let Some(body) = self.emit_traversal(operation, value, traversal)? else {
            return Ok(None);
        };
        let function =
            self.builder.context.intern_function(traversal.source_type, traversal.target_type);
        Ok(Some(self.builder.lambda(function, vec![input], body)))
    }

    fn emit_identity(&mut self, traversal: TraversalContext) -> QueryResult<ElaboratedExpression> {
        let input = self.builder.variable_binder("unchanged", traversal.source_type);
        let value = self.builder.variable(input);
        let body = self.builder.subtype(value, traversal.target_type)?;
        let function =
            self.builder.context.intern_function(traversal.source_type, traversal.target_type);
        Ok(self.builder.lambda(function, vec![input], body))
    }

    fn emit_record_traversal(
        &mut self,
        fields: &[RecordFieldRecipe],
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let Some(source_row) =
            extract_record_row(self.builder.state, self.builder.context, traversal.source_type)?
        else {
            return Ok(None);
        };
        let Some(target_row) =
            extract_record_row(self.builder.state, self.builder.context, traversal.target_type)?
        else {
            return Ok(None);
        };

        let mut updates = Vec::with_capacity(fields.len());
        for field in fields {
            let Some(source_field) = source_row.fields.iter().find(|row| row.label == field.label)
            else {
                return Ok(None);
            };
            let Some(target_field) = target_row.fields.iter().find(|row| row.label == field.label)
            else {
                return Ok(None);
            };
            let accessed = self.builder.record_access(value, field.label.clone(), source_field.id);
            let field_context = TraversalContext {
                source_type: source_field.id,
                target_type: target_field.id,
                function_depth: traversal.function_depth,
            };
            let Some(updated) = self.emit_traversal(&field.operation, accessed, field_context)?
            else {
                return Ok(None);
            };
            updates.push(tree::RecordExpressionUpdate::Leaf {
                label: field.label.clone(),
                expression: updated.expression,
            });
        }
        Ok(Some(self.builder.record_update(value, updates, traversal.target_type)))
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
