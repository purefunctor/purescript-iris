use building_types::QueryResult;
use indexing::DeriveItemId;
use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::entail::ConstraintHead;
use crate::core::{data, normalise};
use crate::error::ErrorKind;
use crate::source::instances::type_item_name;
use crate::state::CheckState;
use crate::tree::EvidenceSlotId;
use crate::types::{Type, TypeId};

pub fn derive_obligations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derive_id: DeriveItemId,
    head: &ConstraintHead,
) -> QueryResult<Vec<(TypeId, EvidenceSlotId)>>
where
    Q: ExternalQueries,
{
    let Some(lowered) = context.lowered.tree.get_derive_item(derive_id) else {
        return Ok(Vec::new());
    };
    let class = type_item_name(context, head.class)?;
    let Some((&last, preceding)) = head.arguments.split_last() else {
        state.insert_error(ErrorKind::CannotDerive { class });
        return Ok(Vec::new());
    };

    let obligations = if lowered.newtype {
        let Some(field) = data::derived_newtype_field(state, context, last)? else {
            state.insert_error(ErrorKind::CannotDerive { class });
            return Ok(Vec::new());
        };
        let arguments = preceding.iter().copied().chain([field]).collect_vec();
        vec![constraint_type(context, head.class, &head.kind_arguments, &arguments)]
    } else if context.known.newtype == Some(head.class) {
        Vec::new()
    } else if context.known.eq == Some(head.class) || context.known.ord == Some(head.class) {
        let Some((_, _, fields)) = data::data_fields(state, context, last)? else {
            state.insert_error(ErrorKind::CannotDerive { class });
            return Ok(Vec::new());
        };
        let lifted = if context.known.eq == Some(head.class) {
            context.known.eq1
        } else {
            context.known.ord1
        };
        let mut obligations = Vec::with_capacity(fields.len());
        for field in fields {
            field_obligations(state, context, (head.class, lifted), field, &mut obligations);
        }
        obligations
    } else {
        state.insert_error(ErrorKind::UnsupportedFeature { feature: "deriving this class" });
        Vec::new()
    };

    let obligations =
        obligations.into_iter().map(|constraint| (constraint, state.emit_wanted(constraint)));
    Ok(obligations.collect_vec())
}

fn constraint_type<Q>(
    context: &CheckContext<Q>,
    class: Item,
    kind_arguments: &[TypeId],
    arguments: &[TypeId],
) -> TypeId
where
    Q: ExternalQueries,
{
    let constructor = context.intern_constructor(class);
    let constructor = kind_arguments.iter().fold(constructor, |function, &argument| {
        context.intern_kind_application(function, argument)
    });
    context.intern_application_list(constructor, arguments)
}

fn field_obligations<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (class, lifted): (Item, Option<Item>),
    field: TypeId,
    obligations: &mut Vec<TypeId>,
) where
    Q: ExternalQueries,
{
    let normalised = normalise::normalise(state, context, field);
    let Type::Application(function, argument) = context.lookup_type(normalised) else {
        obligations.push(constraint_type(context, class, &[], &[field]));
        return;
    };
    let function = normalise::normalise(state, context, function);

    if function == context.types.record {
        let row = normalise::row_parts(state, context, argument);
        if row.tail.is_none() {
            for row_field in row.fields.iter() {
                field_obligations(state, context, (class, lifted), row_field.id, obligations);
            }
            return;
        }
    }

    if let Some(lifted) = lifted
        && is_type_constructor_variable(state, context, function)
    {
        obligations.push(constraint_type(context, lifted, &[], &[function]));
        obligations.push(constraint_type(context, class, &[], &[argument]));
        return;
    }

    obligations.push(constraint_type(context, class, &[], &[field]));
}

fn is_type_constructor_variable<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    function: TypeId,
) -> bool
where
    Q: ExternalQueries,
{
    let kind = match context.lookup_type(function) {
        Type::Rigid(rigid_id) => context.lookup_rigid(rigid_id).kind,
        Type::Unification(unification_id) => state.unification(unification_id).kind,
        _ => return false,
    };
    let parts = normalise::function_parts(state, context, kind);
    let parts = parts.map(|(argument, result)| {
        (
            normalise::normalise(state, context, argument),
            normalise::normalise(state, context, result),
        )
    });
    parts == Some((context.types.type_, context.types.type_))
}
