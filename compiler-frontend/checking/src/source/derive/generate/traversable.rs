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
    Traversable,
    Bitraversable,
}

#[derive(Clone, Copy)]
enum Mappings<T> {
    Traversable(T),
    Bitraversable { first: T, second: T },
}

impl Mappings<ElaboratedExpression> {
    fn mapping_for(self, parameter: TraversalParameter) -> Option<ElaboratedExpression> {
        match (self, parameter) {
            (Mappings::Traversable(function), TraversalParameter::First) => Some(function),
            (Mappings::Bitraversable { first, .. }, TraversalParameter::First) => Some(first),
            (Mappings::Bitraversable { second, .. }, TraversalParameter::Second) => Some(second),
            (Mappings::Traversable(_), TraversalParameter::Second) => None,
        }
    }
}

struct DecodedTraversalMember {
    member: ResolvedMember,
    renaming: Arc<RigidRenaming>,
    abstractions: Vec<signature::SkolemisedAbstraction>,
    implementation_type: TypeId,
    function_type: TypeId,
    mappings: Mappings<TypeId>,
    effect: TypeId,
    data_file: files::FileId,
    source: InstantiatedDataType,
    target: InstantiatedDataType,
    result_type: TypeId,
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
        // Decode the operation member into its effectful transformations and
        // structural source, together with its effect-wrapped structural target.
        //
        // List
        //
        //   data List a = Nil | Cons a (List a)
        //
        //   traverse :: (a -> m b) -> List a -> m (List b)
        //
        // Pair
        //
        //   data Pair a b = Pair a b
        //
        //   bitraverse ::
        //     (a -> m c) -> (b -> m d) -> Pair a b -> m (Pair c d)
        let argument_count = match traversal {
            TraversalKind::Traversable => 2,
            TraversalKind::Bitraversable => 3,
        };
        let signature = signature::expect_term_signature(
            state,
            context,
            member.implementation_type,
            argument_count,
        )?;
        let arguments = signature.arguments().collect_vec();
        let signature::SkolemisedSignature { renaming, abstractions, result } = signature;

        // Separate the transformations from the structural source.
        //
        // List
        //
        //   mappings = Traversable (a -> m b)
        //   sourceType = List a
        //
        // Pair
        //
        //   mappings = Bitraversable (a -> m c) (b -> m d)
        //   sourceType = Pair a b
        let (source_type, mappings) = match (traversal, arguments.as_slice()) {
            (TraversalKind::Traversable, [mapping, source]) => {
                (*source, Mappings::Traversable(*mapping))
            }
            (TraversalKind::Bitraversable, [first, second, source]) => {
                (*source, Mappings::Bitraversable { first: *first, second: *second })
            }
            _ => return Ok(None),
        };

        // Separate the applicative constructor from the structural target. Instantiate
        // constructor fields against that target, not the effect-wrapped result.
        //
        // List
        //
        //   result = m (List b)
        //   effect = m
        //   targetType = List b
        //
        // Pair
        //
        //   result = m (Pair c d)
        //   effect = m
        //   targetType = Pair c d
        let Some((effect, target_type)) =
            toolkit::decompose_type_application(state, context, result)?
        else {
            return Ok(None);
        };

        // Retain the source and target arguments for constructor-field instantiation.
        //
        // List
        //
        //   sourceArguments = [a]
        //   targetArguments = [b]
        //
        // Pair
        //
        //   sourceArguments = [a, b]
        //   targetArguments = [c, d]
        let (_, source_arguments) = toolkit::extract_all_applications(state, context, source_type)?;
        let (_, target_arguments) = toolkit::extract_all_applications(state, context, target_type)?;
        let function_arguments = arguments.iter().copied();
        let function_type = context.intern_function_iter(function_arguments, result);

        Ok(Some(DecodedTraversalMember {
            implementation_type: member.implementation_type,
            member,
            renaming,
            abstractions,
            function_type,
            mappings,
            effect,
            data_file,
            source: InstantiatedDataType {
                type_id: source_type,
                constructor_arguments: source_arguments,
            },
            target: InstantiatedDataType {
                type_id: target_type,
                constructor_arguments: target_arguments,
            },
            result_type: result,
        }))
    }
}

pub(super) fn generate_traversal_members<Q>(
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
    let (operation, sequence) = match traversal {
        TraversalKind::Traversable => (context.known_terms.traverse, context.known_terms.sequence),
        TraversalKind::Bitraversable => {
            (context.known_terms.bitraverse, context.known_terms.bisequence)
        }
    };
    let (Some(operation), Some(sequence)) = (operation, sequence) else {
        return Ok(None);
    };

    let Some(class) =
        toolkit::lookup_file_class(state, context, result.class_file, result.class_id)?
    else {
        return Ok(None);
    };

    // Traversable and Bitraversable dictionaries require an operation and a sequence
    // member. Resolve both by known identity because declaration order is not part
    // of the class contract, and reject the dictionary if either one is absent.
    let mut operation_generated = false;
    let mut sequence_generated = false;
    let mut members = Vec::with_capacity(class.members.len());
    for class_member in class.members.iter() {
        let resolution = (result.class_file, class_member.item_id);
        let member = if resolution == operation {
            operation_generated = true;
            generate_operation_member(
                state,
                context,
                result,
                instance_arguments,
                recipe,
                traversal,
            )?
        } else if resolution == sequence {
            sequence_generated = true;
            generate_sequence_member(state, context, result, instance_arguments, traversal)?
        } else {
            return Ok(None);
        };
        let Some(member) = member else {
            return Ok(None);
        };
        members.push(member);
    }

    if !operation_generated || !sequence_generated {
        return Ok(None);
    }
    Ok(Some(members))
}

fn generate_operation_member<Q>(
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
    let DeriveStrategy::VarianceConstraints { data_file, .. } = result.strategy else {
        return Ok(None);
    };
    let resolution = match traversal {
        TraversalKind::Traversable => context.known_terms.traverse,
        TraversalKind::Bitraversable => context.known_terms.bitraverse,
    };
    let Some(resolution) = resolution else { return Ok(None) };

    state.with_implication(|state| {
        let Some(member) =
            resolve_known_member(state, context, result, instance_arguments, resolution)?
        else {
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

fn generate_sequence_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    traversal: TraversalKind,
) -> QueryResult<Option<tree::InstanceMember>>
where
    Q: ExternalQueries,
{
    let (operation, resolution) = match traversal {
        TraversalKind::Traversable => (context.known_terms.traverse, context.known_terms.sequence),
        TraversalKind::Bitraversable => {
            (context.known_terms.bitraverse, context.known_terms.bisequence)
        }
    };
    let (Some(operation), Some(resolution)) = (operation, resolution) else {
        return Ok(None);
    };

    state.with_implication(|state| {
        let Some(member) =
            resolve_known_member(state, context, result, instance_arguments, resolution)?
        else {
            return Ok(None);
        };
        let signature =
            signature::expect_term_signature(state, context, member.implementation_type, 1)?;
        let arguments = signature.arguments().collect_vec();
        let signature::SkolemisedSignature { renaming, abstractions, result: body_type } =
            signature;
        let [source_type] = arguments.as_slice() else {
            return Ok(None);
        };

        // Sequence delegates to the operation member with identity for each traversed
        // parameter, avoiding a second structural traversal implementation.
        //
        // List
        //
        //   data List a = Nil | Cons a (List a)
        //
        //   sequence value = traverse identity value
        //
        // Pair
        //
        //   data Pair a b = Pair a b
        //
        //   bisequence value = bitraverse identity identity value
        //
        // Recover the instantiated parameter types from the source so each
        // generated identity has the type expected by the operation member.
        //
        // List
        //
        //   sourceType    = List (m a)
        //   identityTypes = [m a]
        //
        // Pair
        //
        //   sourceType    = Pair (m a) (m b)
        //   identityTypes = [m a, m b]
        let (_, source_arguments) =
            toolkit::extract_all_applications(state, context, *source_type)?;
        let identity_types = match (traversal, source_arguments.as_slice()) {
            (TraversalKind::Traversable, [.., ApplicationArgument::Type(effect)]) => vec![*effect],
            (
                TraversalKind::Bitraversable,
                [.., ApplicationArgument::Type(first), ApplicationArgument::Type(second)],
            ) => vec![*first, *second],
            _ => return Ok(None),
        };

        let abstractions = equations::bind_signature_abstractions(state, &abstractions);

        let body = state.with_source_type_renaming(&renaming, |state| {
            let mut builder = DerivedTreeBuilder::new(state, context, result.derive_id);
            let value = builder.variable_binder("value", *source_type);
            let value_expression = builder.variable(value);

            // Apply the eta-expanded identities before the source value.
            //
            // List
            //
            //   traverse (\effect0 -> effect0) value
            //
            // Pair
            //
            //   bitraverse
            //     (\effect0 -> effect0)
            //     (\effect1 -> effect1)
            //     value
            let mut applied = builder.term_reference(operation)?;
            for (index, identity_type) in identity_types.into_iter().enumerate() {
                let name = format_smolstr!("effect{index}");
                let input = builder.variable_binder(&name, identity_type);
                let identity_value = builder.variable(input);
                let identity_type = context.intern_function(identity_type, identity_type);
                let identity = builder.lambda(identity_type, vec![input], identity_value);
                let Some(application) = builder.apply(applied, identity)? else {
                    return Ok(None);
                };
                applied = application;
            }
            let Some(applied) = builder.apply(applied, value_expression)? else {
                return Ok(None);
            };
            let body = builder.subtype(applied, body_type)?;
            let function_type = context.intern_function(*source_type, body_type);
            Ok(Some(builder.lambda(function_type, vec![value], body)))
        })?;
        let Some(body) = body else { return Ok(None) };

        Ok(Some(generated_member(
            result.derive_id,
            (member.file_id, member.item_id),
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
        Mappings::Traversable(mapping) => {
            let function = builder.variable_binder("function", mapping);
            let expression = builder.variable(function);
            (vec![function], Mappings::Traversable(expression))
        }
        Mappings::Bitraversable { first, second } => {
            let first = builder.variable_binder("firstFunction", first);
            let second = builder.variable_binder("secondFunction", second);
            let expressions = Mappings::Bitraversable {
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

    let body = builder.case(member.result_type, vec![value_expression], alternatives);
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

    // Bind each source field and inspect its recipe. A field with a traversal
    // operation is active and emits an effect; every other field remains fixed.
    //
    // List
    //
    //   data List a = Nil | Cons a (List a)
    //
    // Cons
    //
    //   head :: a
    //   tail :: List a
    //
    //   headEffect = function head
    //   tailEffect = traverse function tail
    let mut emitter =
        EffectTraversalEmitter { builder, mapping_expressions, effect: member.effect };
    let mut source_binders = Vec::with_capacity(source_fields.len());
    let mut reconstruction_values = Vec::with_capacity(source_fields.len());
    let mut traversed_fields = Vec::with_capacity(source_fields.len());

    for (index, (source, target, operation)) in
        izip!(&source_fields, &target_fields, &constructor.fields).enumerate()
    {
        let source_binder =
            emitter.builder.variable_binder(&format_smolstr!("field{index}"), *source);
        let source_value = emitter.builder.variable(source_binder);
        source_binders.push(source_binder);

        if let Some(operation) = operation {
            let traversal = TraversalContext { source_type: *source, target_type: *target };
            let Some(effect) = emitter.emit_effect(operation, source_value, traversal)? else {
                return Ok(None);
            };
            let result_binder =
                emitter.builder.variable_binder(&format_smolstr!("field{index}Result"), *target);
            reconstruction_values.push(emitter.builder.variable(result_binder));
            traversed_fields.push((result_binder, effect));
        } else {
            reconstruction_values.push(source_value);
        }
    }

    // Match the source constructor using the field binders allocated above.
    //
    // Nil
    //
    //   pattern = Nil
    //
    // Cons
    //
    //   pattern = Cons head tail
    let pattern = emitter.builder.constructor_pattern(
        "constructor",
        member.source.type_id,
        (member.data_file, constructor.constructor_id),
        source_binders,
    );

    // Rebuild the target constructor. Applicative reconstruction supplies result
    // binders for active fields; fixed fields reuse their source values directly.
    //
    // Nil
    //
    //   reconstructed = Nil
    //
    // Cons
    //
    //   reconstructed = Cons headResult tailResult
    let mut reconstructed =
        emitter.builder.term_reference((member.data_file, constructor.constructor_id))?;
    for value in reconstruction_values {
        let Some(applied) = emitter.builder.apply(reconstructed, value)? else {
            return Ok(None);
        };
        reconstructed = applied;
    }
    let reconstructed = emitter.builder.subtype(reconstructed, member.target.type_id)?;

    // Lift the reconstructed target over the field effects.
    let Some(body) =
        emitter.lift_reconstruction(member.target.type_id, traversed_fields, reconstructed)?
    else {
        return Ok(None);
    };
    Ok(Some(emitter.builder.alternative(vec![pattern], body)))
}

#[derive(Clone, Copy)]
struct TraversalContext {
    source_type: TypeId,
    target_type: TypeId,
}

struct EffectTraversalEmitter<'builder, 'state, 'context, 'queries, Q: ExternalQueries> {
    builder: &'builder mut DerivedTreeBuilder<'state, 'context, 'queries, Q>,
    mapping_expressions: Mappings<ElaboratedExpression>,
    effect: TypeId,
}

impl<Q> EffectTraversalEmitter<'_, '_, '_, '_, Q>
where
    Q: ExternalQueries,
{
    fn emit_effect(
        &mut self,
        operation: &TraversalOperation,
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        match operation {
            TraversalOperation::Parameter { parameter } => {
                // A parameter is a traversal leaf whose function already introduces the
                // applicative effect. Apply it directly to the constructor field.
                //
                // Identity
                //
                //   newtype Identity a = Identity a
                //
                //   field    :: a
                //   target   :: b
                //   function :: a -> m b
                //
                //   function field :: m b
                let Some(mapping_expression) = self.mapping_expressions.mapping_for(*parameter)
                else {
                    return Ok(None);
                };
                let Some(mapped) = self.builder.apply(mapping_expression, value)? else {
                    return Ok(None);
                };
                let effect_type = self.effect_type(traversal.target_type);
                Ok(Some(self.builder.subtype(mapped, effect_type)?))
            }
            TraversalOperation::UnaryApplication { argument, .. } => {
                // Traverse delegates a unary type constructor to its Traversable instance.
                // Supply the effectful transformation between its applied source and target.
                //
                // Compose
                //
                //   newtype Compose f g a = Compose (f (g a))
                //
                //   function :: a -> m b
                //
                //   field    :: f (g a)
                //   target   :: f (g b)
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

                // Generate the transformation for the delegated traverse call. Its argument
                // operation recursively traverses the inner `g` application.
                //
                // Compose
                //
                //   sourceArgument :: g a
                //   targetArgument :: g b
                //
                //   transformer :: g a -> m (g b)
                //   transformer = \element -> traverse function element
                let argument_context =
                    TraversalContext { source_type: source_argument, target_type: target_argument };
                let Some(transformer) = self.emit_transformer(argument, argument_context)? else {
                    return Ok(None);
                };

                // Applying the transformer fixes the element types. Applying the field fixes
                // the fresh constructor variable to `f`, specializing its wanted evidence.
                //
                //   traverse ::
                //     forall t a b m.
                //     Traversable t => Applicative m =>
                //     (a -> m b) -> t a -> m (t b)
                //
                //   ?t := f
                //
                //   traverse transformer field :: m (f (g b))
                let Some(traverse) = self.builder.context.known_terms.traverse else {
                    return Ok(None);
                };
                let traverse = self.builder.term_reference(traverse)?;
                let Some(traverse) = self.builder.apply(traverse, transformer)? else {
                    return Ok(None);
                };
                let Some(traversed) = self.builder.apply(traverse, value)? else {
                    return Ok(None);
                };
                let effect_type = self.effect_type(traversal.target_type);
                Ok(Some(self.builder.subtype(traversed, effect_type)?))
            }
            TraversalOperation::BinaryApplication { arguments, .. } => {
                // Bitraverse delegates both arguments of a binary type constructor. See
                // `emit_binary_effect` for the staged construction of its transformers.
                let (first, second) = arguments.operations();
                self.emit_binary_effect(first, second, value, traversal)
            }
            TraversalOperation::Record { fields } => {
                // Record traversal combines the effects of its active field updates.
                // See `emit_record_effect` for applicative reconstruction.
                self.emit_record_effect(fields, value, traversal)
            }
            // Function operations are rejected during variance analysis.
            TraversalOperation::Function { .. } => Ok(None),
        }
    }

    fn emit_transformer(
        &mut self,
        operation: &TraversalOperation,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // Traverse and bitraverse need effectful transformations for contained values.
        // Bind one source value, emit its effect, and return the resulting lambda.
        //
        //   source  :: a
        //   target  :: b
        //   element :: a
        //
        //   body :: m b
        //   body = emit_effect operation element
        //
        //   transformer :: a -> m b
        //   transformer = \element -> body
        let input = self.builder.variable_binder("element", traversal.source_type);
        let value = self.builder.variable(input);
        let Some(body) = self.emit_effect(operation, value, traversal)? else {
            return Ok(None);
        };

        let effect_type = self.effect_type(traversal.target_type);
        let function_type =
            self.builder.context.intern_function(traversal.source_type, effect_type);
        Ok(Some(self.builder.lambda(function_type, vec![input], body)))
    }

    fn emit_binary_effect(
        &mut self,
        first: Option<&TraversalOperation>,
        second: Option<&TraversalOperation>,
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // Decompose both arguments of the binary type application.
        //
        //   function :: a -> m b
        //
        // Duplicate
        //
        //   newtype Duplicate p a = Duplicate (p a a)
        //
        //   source :: p a a
        //   target :: p b b
        //
        // LeftDuplicate
        //
        //   newtype LeftDuplicate p a = LeftDuplicate (p a Int)
        //
        //   source :: p a Int
        //   target :: p b Int
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

        // Generate the effectful transformation for the first argument. If the
        // derived parameter is absent there, lift the unchanged value with pure.
        //
        // Duplicate and LeftDuplicate
        //
        //   firstTransformer :: a -> m b
        //   firstTransformer = function
        let first_context =
            TraversalContext { source_type: source_first, target_type: target_first };
        let first_transformer = if let Some(first) = first {
            let Some(transformer) = self.emit_transformer(first, first_context)? else {
                return Ok(None);
            };
            transformer
        } else {
            let Some(transformer) = self.emit_pure_transformer(first_context)? else {
                return Ok(None);
            };
            transformer
        };

        // Generate the effectful transformation for the second argument. If the
        // derived parameter is absent there, lift the unchanged value with pure.
        //
        // Duplicate
        //
        //   secondTransformer :: a -> m b
        //   secondTransformer = function
        //
        // LeftDuplicate
        //
        //   secondTransformer :: Int -> m Int
        //   secondTransformer = \unchanged -> pure unchanged
        let second_context =
            TraversalContext { source_type: source_second, target_type: target_second };
        let second_transformer = match second {
            Some(second) => {
                let Some(transformer) = self.emit_transformer(second, second_context)? else {
                    return Ok(None);
                };
                transformer
            }
            None => {
                let Some(transformer) = self.emit_pure_transformer(second_context)? else {
                    return Ok(None);
                };
                transformer
            }
        };

        // Specialize bitraverse through its transformers and source field.
        //
        // Duplicate
        //
        //   bitraverse firstTransformer secondTransformer field :: m (p b b)
        //
        // LeftDuplicate
        //
        //   bitraverse firstTransformer secondTransformer field :: m (p b Int)
        let Some(bitraverse) = self.builder.context.known_terms.bitraverse else {
            return Ok(None);
        };
        let bitraverse = self.builder.term_reference(bitraverse)?;
        let Some(bitraverse) = self.builder.apply(bitraverse, first_transformer)? else {
            return Ok(None);
        };
        let Some(bitraverse) = self.builder.apply(bitraverse, second_transformer)? else {
            return Ok(None);
        };
        let Some(traversed) = self.builder.apply(bitraverse, value)? else {
            return Ok(None);
        };
        let effect_type = self.effect_type(traversal.target_type);
        Ok(Some(self.builder.subtype(traversed, effect_type)?))
    }

    fn emit_pure_transformer(
        &mut self,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // Bitraverse still needs an effectful transformer for a binary argument that omits
        // the derived parameter, so lift its value with pure.
        //
        // LeftDuplicate
        //
        //   sourceSecond :: Int
        //   targetSecond :: Int
        //
        //   transformer :: Int -> m Int
        //   transformer = \unchanged -> pure unchanged
        let input = self.builder.variable_binder("unchanged", traversal.source_type);
        let value = self.builder.variable(input);
        let value = self.builder.subtype(value, traversal.target_type)?;
        let Some(pure) = self.emit_pure(value, traversal.target_type)? else {
            return Ok(None);
        };
        let effect_type = self.effect_type(traversal.target_type);
        let function_type =
            self.builder.context.intern_function(traversal.source_type, effect_type);
        Ok(Some(self.builder.lambda(function_type, vec![input], pure)))
    }

    fn emit_record_effect(
        &mut self,
        fields: &[RecordFieldRecipe],
        value: ElaboratedExpression,
        traversal: TraversalContext,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // Recover the source and target rows used to type each access and update.
        //
        // Record
        //
        //   newtype Record a = Record { zeta :: a, fixed :: Int, alpha :: a }
        //
        //   function :: a -> m b
        //
        //   source :: { zeta :: a, fixed :: Int, alpha :: a }
        //   target :: { zeta :: b, fixed :: Int, alpha :: b }
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

        // The recipe lists active fields in canonical row order. Pair each field effect
        // with a result binder; fields absent from the recipe remain unchanged.
        //
        // Record
        //
        //   alphaEffect = function source.alpha
        //   zetaEffect = function source.zeta
        let mut updates = Vec::with_capacity(fields.len());
        let mut traversed_fields = Vec::with_capacity(fields.len());
        for (index, field) in fields.iter().enumerate() {
            let Some(source_field) = source_row.fields.iter().find(|row| row.label == field.label)
            else {
                return Ok(None);
            };
            let Some(target_field) = target_row.fields.iter().find(|row| row.label == field.label)
            else {
                return Ok(None);
            };
            let accessed = self.builder.record_access(value, field.label.clone(), source_field.id);
            let field_context =
                TraversalContext { source_type: source_field.id, target_type: target_field.id };
            let Some(effect) = self.emit_effect(&field.operation, accessed, field_context)? else {
                return Ok(None);
            };
            let binder = self
                .builder
                .variable_binder(&format_smolstr!("recordField{index}"), target_field.id);
            let expression = self.builder.variable(binder);
            updates.push(tree::RecordExpressionUpdate::Leaf {
                label: field.label.clone(),
                expression: expression.expression,
            });
            traversed_fields.push((binder, effect));
        }

        // Rebuild the target record from the active result binders. Applicative
        // reconstruction supplies those binders in the same order as their effects.
        //
        // Record
        //
        //   reconstructed =
        //     source { alpha = alphaResult, zeta = zetaResult }
        let reconstructed = self.builder.record_update(value, updates, traversal.target_type);
        self.lift_reconstruction(traversal.target_type, traversed_fields, reconstructed)
    }

    fn lift_reconstruction(
        &mut self,
        target_type: TypeId,
        traversed_fields: Vec<(tree::BinderId, ElaboratedExpression)>,
        reconstructed: ElaboratedExpression,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        // Lift constructor or record reconstruction over its field effects. With no
        // active fields, lift the reconstruction directly with pure.
        //
        // List
        //
        //   data List a = Nil | Cons a (List a)
        //
        // Nil
        //
        //   reconstructed = Nil
        //   traversedFields = []
        //
        //   pure Nil :: m (List b)
        let mut traversed_fields = traversed_fields.into_iter();
        let Some((first_binder, first_effect)) = traversed_fields.next() else {
            return self.emit_pure(reconstructed, target_type);
        };

        // Separate the result binders from the remaining effects. Preserve each
        // binder's association with its effect and their shared order.
        //
        // Cons
        //
        //   headEffect :: m b
        //   headEffect = function head
        //
        //   tailEffect :: m (List b)
        //   tailEffect = traverse function tail
        let mut binders = vec![first_binder];
        let mut remaining_effects = Vec::new();
        for (binder, effect) in traversed_fields {
            binders.push(binder);
            remaining_effects.push(effect);
        }

        // Abstract the reconstructed target over its active result binders.
        //
        // Cons
        //
        //   reconstruction :: b -> List b -> List b
        //   reconstruction = \headResult tailResult -> Cons headResult tailResult
        let binder_types =
            binders.iter().map(|binder| self.builder.state.checked.tree[*binder].type_id);
        let reconstruction_type =
            self.builder.context.intern_function_iter(binder_types, target_type);
        let reconstruction = self.builder.lambda(reconstruction_type, binders, reconstructed);

        // Introduce the first effect by mapping the reconstruction function over it.
        //
        // Cons
        //
        //   reconstruction <$> headEffect :: m (List b -> List b)
        let Some(map) = self.builder.context.known_terms.map else {
            return Ok(None);
        };
        let map = self.builder.term_reference(map)?;
        let Some(map) = self.builder.apply(map, reconstruction)? else {
            return Ok(None);
        };
        let Some(mut accumulated) = self.builder.apply(map, first_effect)? else {
            return Ok(None);
        };

        // Apply remaining effects from left to right in their established order.
        //
        // Cons
        //
        //   reconstruction <$> headEffect <*> tailEffect :: m (List b)
        for effect in remaining_effects {
            let Some(apply) = self.builder.context.known_terms.apply else {
                return Ok(None);
            };
            let apply = self.builder.term_reference(apply)?;
            let Some(apply) = self.builder.apply(apply, accumulated)? else {
                return Ok(None);
            };
            let Some(applied) = self.builder.apply(apply, effect)? else {
                return Ok(None);
            };
            accumulated = applied;
        }

        let effect_type = self.effect_type(target_type);
        Ok(Some(self.builder.subtype(accumulated, effect_type)?))
    }

    fn emit_pure(
        &mut self,
        value: ElaboratedExpression,
        target_type: TypeId,
    ) -> QueryResult<Option<ElaboratedExpression>> {
        let Some(pure) = self.builder.context.known_terms.pure else {
            return Ok(None);
        };
        let pure = self.builder.term_reference(pure)?;
        let Some(pure) = self.builder.apply(pure, value)? else {
            return Ok(None);
        };
        let effect_type = self.effect_type(target_type);
        Ok(Some(self.builder.subtype(pure, effect_type)?))
    }

    fn effect_type(&self, target_type: TypeId) -> TypeId {
        self.builder.context.intern_application(self.effect, target_type)
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
