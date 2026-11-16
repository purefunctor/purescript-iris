use building_types::QueryResult;
use indexing::TermItemId;
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::context::{CheckContext, Item};
use crate::core::{entail, fold};
use crate::state::CheckState;
use crate::types::TypeId;
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
        return Ok(state.checked.data.get(&type_id).map(CheckedData::clone));
    }
    let checked = context.checked_dependency(file_id)?;
    Ok(checked.data.get(&type_id).map(CheckedData::clone))
}

pub fn data_fields<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<Option<(Item, CheckedData, Vec<TypeId>)>>
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

    let fields = data.constructors.iter().flat_map(|constructor| constructor.fields.iter());
    let fields = fields.map(|&field| fold::substitute_variables(context, &substitution, field));
    let fields = fields.collect_vec();
    Ok(Some((head.class, data, fields)))
}

pub fn newtype_field<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    type_id: TypeId,
) -> QueryResult<Option<NewtypeField>>
where
    Q: ExternalQueries,
{
    let Some(((file_id, _), data, fields)) = data_fields(state, context, type_id)? else {
        return Ok(None);
    };
    let (true, [constructor], &[field]) =
        (data.newtype, data.constructors.as_ref(), fields.as_slice())
    else {
        return Ok(None);
    };
    Ok(Some(NewtypeField { constructor: (file_id, constructor.item), field }))
}
