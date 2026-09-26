use building_types::QueryResult;
use smol_str::format_smolstr;

use crate::context::{CheckContext, KnownGeneric};
use crate::core::{ApplicationArgument, TypeId, normalise, toolkit};
use crate::source::derive::builder::DerivedTreeBuilder;
use crate::source::derive::{field, tools};
use crate::source::terms::ElaboratedExpression;
use crate::state::CheckState;
use crate::{ExternalQueries, tree};

use super::{
    DeriveHeadResult, DeriveStrategy, ResolvedMember, generated_member, resolve_known_member,
};

struct GenericMember {
    member: ResolvedMember,
    argument_type: TypeId,
    result_type: TypeId,
}

struct GenericRepresentation {
    derived_type: TypeId,
    suffix_types: Vec<TypeId>,
    constructors: Vec<GenericConstructor>,
}

struct GenericConstructor {
    resolution: (files::FileId, indexing::TermItemId),
    fields: Vec<TypeId>,
    representation_type: TypeId,
    fields_representation_type: TypeId,
}

pub(super) fn generate_generic_members<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
) -> QueryResult<Option<Vec<tree::InstanceMember>>>
where
    Q: ExternalQueries,
{
    let Some(known) = context.known_generic.as_ref() else {
        return Ok(None);
    };
    let Some(representation) =
        instantiate_representation(state, context, result, instance_arguments, known)?
    else {
        return Ok(None);
    };
    let Some(to) = resolve_generic_member(state, context, result, instance_arguments, known.to)?
    else {
        return Ok(None);
    };
    let Some(from) =
        resolve_generic_member(state, context, result, instance_arguments, known.from)?
    else {
        return Ok(None);
    };

    let Some(to_body) = generate_to(state, context, result, known, &representation, &to)? else {
        return Ok(None);
    };
    let Some(from_body) = generate_from(state, context, result, known, &representation, &from)?
    else {
        return Ok(None);
    };

    let to = generated_member(
        result.derive_id,
        (to.member.file_id, to.member.item_id),
        to.member.implementation_type,
        Vec::new(),
        to_body,
    );
    let from = generated_member(
        result.derive_id,
        (from.member.file_id, from.member.item_id),
        from.member.implementation_type,
        Vec::new(),
        from_body,
    );
    Ok(Some(vec![to, from]))
}

fn resolve_generic_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    resolution: (files::FileId, indexing::TermItemId),
) -> QueryResult<Option<GenericMember>>
where
    Q: ExternalQueries,
{
    let Some(member) =
        resolve_known_member(state, context, result, instance_arguments, resolution)?
    else {
        return Ok(None);
    };
    let toolkit::InspectFunction { arguments, result: result_type } =
        toolkit::inspect_function(state, context, member.implementation_type)?;
    let [argument_type] = arguments.as_slice() else {
        return Ok(None);
    };
    Ok(Some(GenericMember { member, argument_type: *argument_type, result_type }))
}

fn instantiate_representation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    instance_arguments: &[ApplicationArgument],
    known: &KnownGeneric,
) -> QueryResult<Option<GenericRepresentation>>
where
    Q: ExternalQueries,
{
    let DeriveStrategy::Generic { data_file, data_id } = result.strategy else {
        return Ok(None);
    };
    let [ApplicationArgument::Type(derived_type), ApplicationArgument::Type(representation_type)] =
        instance_arguments
    else {
        return Ok(None);
    };

    let constructor_ids = tools::lookup_data_constructors(context, data_file, data_id)?;
    if constructor_ids.is_empty() {
        let representation_type = normalise::expand(state, context, *representation_type)?;
        let no_constructors = normalise::expand(state, context, known.no_constructors)?;
        if representation_type != no_constructors {
            return Ok(None);
        }
        return Ok(Some(GenericRepresentation {
            derived_type: *derived_type,
            suffix_types: Vec::new(),
            constructors: Vec::new(),
        }));
    }

    let Some((suffix_types, constructor_representations)) = split_sum_representations(
        state,
        context,
        known,
        *representation_type,
        constructor_ids.len(),
    )?
    else {
        return Ok(None);
    };
    let (_, data_arguments) = toolkit::extract_all_applications(state, context, *derived_type)?;
    let mut constructors = Vec::with_capacity(constructor_ids.len());
    for (&constructor_id, &constructor_representation) in
        std::iter::zip(&constructor_ids, &constructor_representations)
    {
        let constructor_type =
            toolkit::lookup_file_term(state, context, data_file, constructor_id)?;
        let fields = field::instantiate_constructor_fields(
            state,
            context,
            constructor_type,
            &data_arguments,
        )?;
        let Some(arguments) =
            applied_arguments(state, context, known.constructor, constructor_representation)?
        else {
            return Ok(None);
        };
        let [_, fields_representation_type] = arguments.as_slice() else {
            return Ok(None);
        };
        constructors.push(GenericConstructor {
            resolution: (data_file, constructor_id),
            fields,
            representation_type: constructor_representation,
            fields_representation_type: *fields_representation_type,
        });
    }

    Ok(Some(GenericRepresentation { derived_type: *derived_type, suffix_types, constructors }))
}

fn split_sum_representations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    known: &KnownGeneric,
    representation_type: TypeId,
    constructor_count: usize,
) -> QueryResult<Option<(Vec<TypeId>, Vec<TypeId>)>>
where
    Q: ExternalQueries,
{
    let mut current = representation_type;
    let mut suffix_types = Vec::with_capacity(constructor_count);
    let mut constructors = Vec::with_capacity(constructor_count);
    for position in 0..constructor_count {
        suffix_types.push(current);
        if position + 1 == constructor_count {
            constructors.push(current);
            continue;
        }
        let Some(arguments) = applied_arguments(state, context, known.sum, current)? else {
            return Ok(None);
        };
        let [left, right] = arguments.as_slice() else {
            return Ok(None);
        };
        constructors.push(*left);
        current = *right;
    }
    Ok(Some((suffix_types, constructors)))
}

fn applied_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constructor: TypeId,
    application: TypeId,
) -> QueryResult<Option<Vec<TypeId>>>
where
    Q: ExternalQueries,
{
    let (head, arguments) = toolkit::extract_type_application(state, context, application)?;
    let head = normalise::expand(state, context, head)?;
    let constructor = normalise::expand(state, context, constructor)?;
    Ok((head == constructor).then_some(arguments))
}

fn generate_to<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    known: &KnownGeneric,
    representation: &GenericRepresentation,
    member: &GenericMember,
) -> QueryResult<Option<ElaboratedExpression>>
where
    Q: ExternalQueries,
{
    if representation.constructors.is_empty() {
        return generate_impossible_member(state, context, result, member, known.to);
    }

    let mut builder = DerivedTreeBuilder::new(state, context, result.derive_id);
    let value = builder.variable_binder("representation", member.argument_type);
    let value_expression = builder.variable(value);
    let mut alternatives = Vec::with_capacity(representation.constructors.len());
    for (position, constructor) in representation.constructors.iter().enumerate() {
        let Some(alternative) = emit_to_alternative(
            &mut builder,
            known,
            representation,
            constructor,
            position,
            member.result_type,
        )?
        else {
            return Ok(None);
        };
        alternatives.push(alternative);
    }
    let body = builder.case(member.result_type, vec![value_expression], alternatives);
    Ok(Some(builder.lambda(member.member.implementation_type, vec![value], body)))
}

fn emit_to_alternative<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    known: &KnownGeneric,
    representation: &GenericRepresentation,
    constructor: &GenericConstructor,
    position: usize,
    result_type: TypeId,
) -> QueryResult<Option<tree::CaseAlternative>>
where
    Q: ExternalQueries,
{
    let field_binders = allocate_field_binders(builder, &constructor.fields);
    let Some(fields_pattern) = emit_fields_pattern(
        builder,
        known,
        &constructor.fields,
        &field_binders,
        constructor.fields_representation_type,
    )?
    else {
        return Ok(None);
    };
    let pattern = builder.constructor_pattern(
        "constructor",
        constructor.representation_type,
        known.constructor_value,
        vec![fields_pattern],
    );
    let pattern = wrap_sum_pattern(builder, known, representation, pattern, position);

    let mut body = builder.term_reference(constructor.resolution)?;
    for binder in field_binders {
        let field = builder.variable(binder);
        let Some(applied) = builder.apply(body, field)? else {
            return Ok(None);
        };
        body = applied;
    }
    let body = builder.subtype(body, result_type)?;
    Ok(Some(builder.alternative(vec![pattern], body)))
}

fn generate_from<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    known: &KnownGeneric,
    representation: &GenericRepresentation,
    member: &GenericMember,
) -> QueryResult<Option<ElaboratedExpression>>
where
    Q: ExternalQueries,
{
    if representation.constructors.is_empty() {
        return generate_impossible_member(state, context, result, member, known.from);
    }

    let mut builder = DerivedTreeBuilder::new(state, context, result.derive_id);
    let value = builder.variable_binder("value", member.argument_type);
    let value_expression = builder.variable(value);
    let mut alternatives = Vec::with_capacity(representation.constructors.len());
    for (position, constructor) in representation.constructors.iter().enumerate() {
        let Some(alternative) = emit_from_alternative(
            &mut builder,
            known,
            representation,
            constructor,
            position,
            member.result_type,
        )?
        else {
            return Ok(None);
        };
        alternatives.push(alternative);
    }
    let body = builder.case(member.result_type, vec![value_expression], alternatives);
    Ok(Some(builder.lambda(member.member.implementation_type, vec![value], body)))
}

fn emit_from_alternative<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    known: &KnownGeneric,
    representation: &GenericRepresentation,
    constructor: &GenericConstructor,
    position: usize,
    result_type: TypeId,
) -> QueryResult<Option<tree::CaseAlternative>>
where
    Q: ExternalQueries,
{
    let field_binders = allocate_field_binders(builder, &constructor.fields);
    let pattern = builder.constructor_pattern(
        "constructor",
        representation.derived_type,
        constructor.resolution,
        field_binders.clone(),
    );

    let field_expressions = field_binders.into_iter().map(|binder| builder.variable(binder));
    let field_expressions = field_expressions.collect();
    let Some(fields) = emit_fields_expression(builder, known, field_expressions)? else {
        return Ok(None);
    };
    let constructor_value = builder.term_reference(known.constructor_value)?;
    let Some(mut body) = builder.apply(constructor_value, fields)? else {
        return Ok(None);
    };
    body = builder.subtype(body, constructor.representation_type)?;
    if position + 1 < representation.constructors.len() {
        let in_left = builder.term_reference(known.in_left)?;
        let Some(applied) = builder.apply(in_left, body)? else {
            return Ok(None);
        };
        body = builder.subtype(applied, representation.suffix_types[position])?;
    }
    for outer in (0..position).rev() {
        let in_right = builder.term_reference(known.in_right)?;
        let Some(applied) = builder.apply(in_right, body)? else {
            return Ok(None);
        };
        body = builder.subtype(applied, representation.suffix_types[outer])?;
    }
    let body = builder.subtype(body, result_type)?;
    Ok(Some(builder.alternative(vec![pattern], body)))
}

fn allocate_field_binders<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    fields: &[TypeId],
) -> Vec<tree::BinderId>
where
    Q: ExternalQueries,
{
    let binders = fields.iter().enumerate().map(|(position, &field)| {
        builder.variable_binder(&format_smolstr!("field{position}"), field)
    });
    binders.collect()
}

fn emit_fields_pattern<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    known: &KnownGeneric,
    fields: &[TypeId],
    binders: &[tree::BinderId],
    representation_type: TypeId,
) -> QueryResult<Option<tree::BinderId>>
where
    Q: ExternalQueries,
{
    match (fields, binders) {
        ([], []) => {
            let representation_type =
                normalise::expand(builder.state, builder.context, representation_type)?;
            let no_arguments =
                normalise::expand(builder.state, builder.context, known.no_arguments)?;
            if representation_type != no_arguments {
                return Ok(None);
            }
            Ok(Some(builder.constructor_pattern(
                "noArguments",
                representation_type,
                known.no_arguments_value,
                Vec::new(),
            )))
        }
        ([_], [binder]) => {
            let Some(arguments) = applied_arguments(
                builder.state,
                builder.context,
                known.argument,
                representation_type,
            )?
            else {
                return Ok(None);
            };
            let [_] = arguments.as_slice() else {
                return Ok(None);
            };
            Ok(Some(builder.constructor_pattern(
                "argument",
                representation_type,
                known.argument_value,
                vec![*binder],
            )))
        }
        ([_, remaining_fields @ ..], [binder, remaining_binders @ ..]) => {
            let Some(arguments) = applied_arguments(
                builder.state,
                builder.context,
                known.product,
                representation_type,
            )?
            else {
                return Ok(None);
            };
            let [left, right] = arguments.as_slice() else {
                return Ok(None);
            };
            let Some(argument) =
                applied_arguments(builder.state, builder.context, known.argument, *left)?
            else {
                return Ok(None);
            };
            let [_] = argument.as_slice() else {
                return Ok(None);
            };
            let argument =
                builder.constructor_pattern("argument", *left, known.argument_value, vec![*binder]);
            let Some(remaining) =
                emit_fields_pattern(builder, known, remaining_fields, remaining_binders, *right)?
            else {
                return Ok(None);
            };
            Ok(Some(builder.constructor_pattern(
                "product",
                representation_type,
                known.product_value,
                vec![argument, remaining],
            )))
        }
        _ => Ok(None),
    }
}

fn wrap_sum_pattern<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    known: &KnownGeneric,
    representation: &GenericRepresentation,
    mut pattern: tree::BinderId,
    position: usize,
) -> tree::BinderId
where
    Q: ExternalQueries,
{
    if position + 1 < representation.constructors.len() {
        pattern = builder.constructor_pattern(
            "inLeft",
            representation.suffix_types[position],
            known.in_left,
            vec![pattern],
        );
    }
    for outer in (0..position).rev() {
        pattern = builder.constructor_pattern(
            "inRight",
            representation.suffix_types[outer],
            known.in_right,
            vec![pattern],
        );
    }
    pattern
}

fn emit_fields_expression<Q>(
    builder: &mut DerivedTreeBuilder<'_, '_, '_, Q>,
    known: &KnownGeneric,
    mut fields: Vec<ElaboratedExpression>,
) -> QueryResult<Option<ElaboratedExpression>>
where
    Q: ExternalQueries,
{
    let Some(last) = fields.pop() else {
        return builder.term_reference(known.no_arguments_value).map(Some);
    };
    let argument = builder.term_reference(known.argument_value)?;
    let Some(mut result) = builder.apply(argument, last)? else {
        return Ok(None);
    };
    for field in fields.into_iter().rev() {
        let argument = builder.term_reference(known.argument_value)?;
        let Some(argument) = builder.apply(argument, field)? else {
            return Ok(None);
        };
        let product = builder.term_reference(known.product_value)?;
        let Some(product) = builder.apply(product, argument)? else {
            return Ok(None);
        };
        let Some(product) = builder.apply(product, result)? else {
            return Ok(None);
        };
        result = product;
    }
    Ok(Some(result))
}

fn generate_impossible_member<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    result: &DeriveHeadResult,
    member: &GenericMember,
    operation: (files::FileId, indexing::TermItemId),
) -> QueryResult<Option<ElaboratedExpression>>
where
    Q: ExternalQueries,
{
    let mut builder = DerivedTreeBuilder::new(state, context, result.derive_id);
    let value = builder.variable_binder("value", member.argument_type);
    let value_expression = builder.variable(value);
    let operation = builder.term_reference(operation)?;
    let Some(body) = builder.apply(operation, value_expression)? else {
        return Ok(None);
    };
    let body = builder.subtype(body, member.result_type)?;
    let pattern = builder.wildcard_pattern("impossible", member.argument_type);
    let alternative = builder.alternative(vec![pattern], body);
    let value_expression = builder.variable(value);
    let body = builder.case(member.result_type, vec![value_expression], vec![alternative]);
    Ok(Some(builder.lambda(member.member.implementation_type, vec![value], body)))
}
