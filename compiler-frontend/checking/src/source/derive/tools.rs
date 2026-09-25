use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTypeItemKind, TermItemId, TypeItemId};
use rustc_hash::FxHashMap;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{CheckedClass, Type, TypeId, toolkit};
use crate::error::ErrorKind;
use crate::state::CheckState;

pub fn emit_constraint<Q>(
    context: &CheckContext<Q>,
    state: &mut CheckState,
    class: (FileId, TypeItemId),
    argument: TypeId,
) where
    Q: ExternalQueries,
{
    let class_t = context.queries.intern_type(Type::Constructor(class.0, class.1));
    state.push_wanted(context.intern_application(class_t, argument));
}

pub fn emit_superclass_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    class_file: FileId,
    class_id: TypeItemId,
    arguments: &[TypeId],
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(CheckedClass { type_parameters, superclasses, .. }) =
        toolkit::lookup_file_class(state, context, class_file, class_id)?
    else {
        return Ok(());
    };

    if superclasses.is_empty() {
        return Ok(());
    }

    let mut bindings = FxHashMap::default();
    for (binder_id, &argument) in type_parameters.iter().zip(arguments.iter()) {
        let binder = context.lookup_forall_binder(*binder_id);
        bindings.insert(binder.name, argument);
    }

    for superclass in superclasses.iter() {
        let specialised = SubstituteName::many(state, context, &bindings, superclass.constraint)?;
        state.push_wanted(specialised);
    }

    Ok(())
}

pub fn lookup_data_constructors<Q>(
    context: &CheckContext<Q>,
    data_file: FileId,
    data_id: TypeItemId,
) -> QueryResult<Vec<TermItemId>>
where
    Q: ExternalQueries,
{
    if data_file == context.id {
        Ok(context.indexed.data_constructors(data_id).collect())
    } else {
        let indexed = context.queries.indexed(data_file)?;
        Ok(indexed.data_constructors(data_id).collect())
    }
}

pub fn extract_local_type_constructor<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derived_type: TypeId,
) -> QueryResult<Option<(FileId, TypeItemId)>>
where
    Q: ExternalQueries,
{
    let Some((type_file, type_id)) =
        toolkit::extract_type_constructor(state, context, derived_type)?
    else {
        return Ok(None);
    };
    if type_file != context.id {
        return Ok(None);
    }

    let kind = &context.indexed.items[type_id].kind;
    match kind {
        IndexedTypeItemKind::Data { .. }
        | IndexedTypeItemKind::Newtype { .. }
        | IndexedTypeItemKind::Foreign { .. } => Ok(Some((type_file, type_id))),
        _ => Ok(None),
    }
}

pub fn extract_local_algebraic_data<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derived_type: TypeId,
) -> QueryResult<Option<(FileId, TypeItemId)>>
where
    Q: ExternalQueries,
{
    let Some((data_file, data_id)) = extract_local_type_constructor(state, context, derived_type)?
    else {
        return Ok(None);
    };

    let kind = &context.indexed.items[data_id].kind;
    match kind {
        IndexedTypeItemKind::Data { .. } | IndexedTypeItemKind::Newtype { .. } => {
            Ok(Some((data_file, data_id)))
        }
        _ => Ok(None),
    }
}

pub enum ConstraintReport {
    Solved,
    Unsolved,
}

pub fn solve_and_report_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
) -> QueryResult<ConstraintReport>
where
    Q: ExternalQueries,
{
    let residuals = state.solve_constraints(context)?;
    let report =
        if residuals.is_empty() { ConstraintReport::Solved } else { ConstraintReport::Unsolved };
    for residual in residuals {
        state.checked.evidence.mark_error(residual.evidence.wanted);
        let attached = state.canonical_errors.remove(&residual.key.wanted);
        attached.into_iter().flatten().for_each(|error| state.insert_error(error));

        let given = residual
            .key
            .given
            .iter()
            .map(|given| state.canonicals.type_id(context, *given))
            .collect::<Arc<[_]>>();

        let constraint = state.canonicals.type_id(context, residual.key.wanted);
        state.insert_error(ErrorKind::NoInstanceFound { given, constraint });
    }
    Ok(report)
}
