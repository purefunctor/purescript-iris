use building_types::QueryResult;
use indexing::TermItemId;
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::context::{CheckContext, Item};
use crate::core::entail::{self, ConstraintHead};
use crate::core::{fold, normalise};
use crate::state::CheckState;
use crate::types::{Name, Type, TypeId};
use crate::{CheckedData, ExternalQueries};

pub struct NewtypeField {
    pub constructor: (files::FileId, TermItemId),
    pub field: TypeId,
}

pub fn lookup_data<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): Item,
) -> QueryResult<Option<CheckedData>>
where
    Q: ExternalQueries,
{
    if file_id == context.id {
        return Ok(state.checked.data.get(&type_id).cloned());
    }
    let checked = context.checked_dependency(file_id)?;
    Ok(checked.data.get(&type_id).cloned())
}

pub fn data_fields<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<Option<(ConstraintHead, CheckedData, Vec<TypeId>)>>
where
    Q: ExternalQueries,
{
    let Some((head, data, constructors)) = constructor_fields(state, context, type_id)? else {
        return Ok(None);
    };
    let fields = constructors.into_iter().flat_map(|constructor| constructor.fields).collect_vec();
    Ok(Some((head, data, fields)))
}

pub struct ConstructorFields {
    pub item: TermItemId,
    pub fields: Vec<TypeId>,
}

pub fn constructor_fields<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<Option<(ConstraintHead, CheckedData, Vec<ConstructorFields>)>>
where
    Q: ExternalQueries,
{
    let Some(head) = entail::constraint_head(state, context, type_id) else {
        return Ok(None);
    };
    let Some(data) = lookup_data(state, context, head.class)? else {
        return Ok(None);
    };

    let kinds = data.kind_binders.iter().zip(&head.kind_arguments);
    let parameters = data.parameters.iter().zip(&head.arguments);
    let substitution = kinds
        .chain(parameters)
        .map(|(&binder_id, &argument)| (context.lookup_forall_binder(binder_id).name, argument));
    let substitution = substitution.collect::<FxHashMap<_, _>>();

    let constructors = data.constructors.iter().map(|constructor| {
        let fields = constructor.fields.iter();
        let fields = fields.map(|&field| fold::substitute_variables(context, &substitution, field));
        ConstructorFields { item: constructor.item, fields: fields.collect_vec() }
    });
    let constructors = constructors.collect_vec();
    Ok(Some((head, data, constructors)))
}

pub fn newtype_field<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<Option<NewtypeField>>
where
    Q: ExternalQueries,
{
    let Some((head, data, fields)) = data_fields(state, context, type_id)? else {
        return Ok(None);
    };
    let (true, [constructor], &[field]) =
        (data.newtype, data.constructors.as_ref(), fields.as_slice())
    else {
        return Ok(None);
    };
    let (file_id, _) = head.class;
    Ok(Some(NewtypeField { constructor: (file_id, constructor.item), field }))
}

pub fn derived_newtype_field<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    let Some((head, data, fields)) = data_fields(state, context, type_id)? else {
        return Ok(None);
    };
    let (true, [_], &[field]) = (data.newtype, data.constructors.as_ref(), fields.as_slice())
    else {
        return Ok(None);
    };

    let unapplied = data.parameters.get(head.arguments.len()..).unwrap_or_default();
    let unapplied = unapplied.iter().map(|&binder_id| context.lookup_forall_binder(binder_id).name);
    let unapplied = unapplied.collect_vec();

    let mut current = field;
    for &name in unapplied.iter().rev() {
        let normalised = normalise::normalise(state, context, current);
        let (function, argument) = match context.lookup_type(normalised) {
            Type::Application(function, argument) => (function, argument),
            Type::Function(argument, result) => {
                (context.intern_application(context.types.function, argument), result)
            }
            _ => return Ok(None),
        };
        let Type::Variable(binder_id) = context.lookup_type(argument) else {
            return Ok(None);
        };
        if context.lookup_forall_binder(binder_id).name != name {
            return Ok(None);
        }
        current = function;
    }

    if unapplied.iter().any(|&name| mentions_variable(context, current, name)) {
        return Ok(None);
    }
    Ok(Some(current))
}

pub fn mentions_variable<Q>(context: &CheckContext<Q>, id: TypeId, name: Name) -> bool
where
    Q: ExternalQueries,
{
    if !context.lookup_type_flags(id).has_variable() {
        return false;
    }
    if let Type::Variable(binder_id) = context.lookup_type(id) {
        return context.lookup_forall_binder(binder_id).name == name;
    }
    let mut mentioned = false;
    fold::map_children(context, id, |child| {
        mentioned = mentioned || mentions_variable(context, child, name);
        child
    });
    mentioned
}
