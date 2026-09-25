//! Implements syntax-driven checking rules for synonym detection.
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::TypeItemId;
use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::{CheckedSynonym, Type, TypeId, toolkit};
use crate::error::ErrorKind;
use crate::source::types::application;
use crate::state::CheckState;

#[derive(Debug, Clone, Copy)]
pub struct ParsedSynonym {
    pub file_id: FileId,
    pub type_id: TypeItemId,
    pub kind: TypeId,
    pub arity: usize,
}

pub fn parse_synonym<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    function: lowering::TypeId,
) -> QueryResult<Option<ParsedSynonym>>
where
    Q: ExternalQueries,
{
    let Some(lowering::TypeKind::Constructor { resolution }) =
        context.lowered.tree.get_type_kind(function)
    else {
        return Ok(None);
    };

    let Some((file_id, type_id)) = *resolution else {
        return Ok(None);
    };

    let Some(CheckedSynonym { kind, parameters, .. }) =
        toolkit::lookup_file_synonym(state, context, file_id, type_id)?
    else {
        return Ok(None);
    };

    let arity = parameters.len();
    Ok(Some(ParsedSynonym { file_id, type_id, kind, arity }))
}

pub fn infer_synonym_constructor<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    synonym: ParsedSynonym,
    id: lowering::TypeId,
) -> QueryResult<application::FnTypeKind>
where
    Q: ExternalQueries,
{
    let ParsedSynonym { file_id, type_id, kind, arity } = synonym;

    if arity > 0 {
        if state.defer_expansion {
            let synonym_type = context.queries.intern_type(Type::Constructor(file_id, type_id));
            return Ok((synonym_type, kind));
        }

        state.insert_error(ErrorKind::PartialSynonymApplication { id });
        let unknown = context.unknown("partial synonym application");
        return Ok((unknown, unknown));
    }

    if is_recursive_synonym(context, file_id, type_id)? {
        state.insert_error(ErrorKind::RecursiveSynonymExpansion { file_id, type_id });
    }

    let synonym_type = context.queries.intern_type(Type::Constructor(file_id, type_id));

    Ok((synonym_type, kind))
}

pub fn infer_synonym_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: lowering::TypeId,
    synonym: ParsedSynonym,
    arguments: &[lowering::TypeId],
) -> QueryResult<application::FnTypeKind>
where
    Q: ExternalQueries,
{
    let ParsedSynonym { file_id, type_id, kind, arity } = synonym;

    if arguments.len() < arity {
        if state.defer_expansion {
            return infer_partial_synonym_application(
                state,
                context,
                (file_id, type_id),
                kind,
                arguments,
            );
        }

        state.insert_error(ErrorKind::PartialSynonymApplication { id });
        let unknown = context.unknown("partial synonym application");
        return Ok((unknown, unknown));
    }

    let arguments = arguments.iter().copied().map(application::Argument::Syntax).collect_vec();

    state.with_defer_expansion(|state| {
        check_synonym_application_arguments(state, context, synonym, &arguments)
    })
}

pub fn check_synonym_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    synonym: ParsedSynonym,
    arguments: &[application::FnTypeKind],
) -> QueryResult<application::FnTypeKind>
where
    Q: ExternalQueries,
{
    let arguments = arguments
        .iter()
        .copied()
        .map(|(type_id, kind_id)| application::Argument::Core(type_id, kind_id))
        .collect_vec();

    state.with_defer_expansion(|state| {
        check_synonym_application_arguments(state, context, synonym, &arguments)
    })
}

pub fn try_check_synonym_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): (FileId, TypeItemId),
    kind: TypeId,
    arguments: &[application::FnTypeKind],
) -> QueryResult<Option<application::FnTypeKind>>
where
    Q: ExternalQueries,
{
    let Some(CheckedSynonym { parameters, .. }) =
        toolkit::lookup_file_synonym(state, context, file_id, type_id)?
    else {
        return Ok(None);
    };

    let synonym = ParsedSynonym { file_id, type_id, kind, arity: parameters.len() };
    let checked = check_synonym_application(state, context, synonym, arguments)?;
    Ok(Some(checked))
}

fn check_synonym_application_arguments<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    synonym: ParsedSynonym,
    arguments: &[application::Argument],
) -> QueryResult<application::FnTypeKind>
where
    Q: ExternalQueries,
{
    let ParsedSynonym { file_id, type_id, kind, arity } = synonym;
    debug_assert!(arguments.len() >= arity);

    if is_recursive_synonym(context, file_id, type_id)? {
        state.insert_error(ErrorKind::RecursiveSynonymExpansion { file_id, type_id });
    }

    let function_type = context.queries.intern_type(Type::Constructor(file_id, type_id));
    let (synonym_arguments, excess_arguments) = arguments.split_at(arity);

    let (mut synonym_type, mut synonym_kind) = application::infer_application_arguments(
        state,
        context,
        (function_type, kind),
        synonym_arguments,
        application::Options::SYNONYM,
    )?;

    if !excess_arguments.is_empty() {
        (synonym_type, synonym_kind) = application::infer_application_arguments(
            state,
            context,
            (synonym_type, synonym_kind),
            excess_arguments,
            application::Options::SYNONYM,
        )?;
    }

    Ok((synonym_type, synonym_kind))
}

fn infer_partial_synonym_application<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    (file_id, type_id): (FileId, TypeItemId),
    kind: TypeId,
    arguments: &[lowering::TypeId],
) -> QueryResult<application::FnTypeKind>
where
    Q: ExternalQueries,
{
    let function_type = context.queries.intern_type(Type::Constructor(file_id, type_id));
    let arguments = arguments.iter().copied().map(application::Argument::Syntax).collect_vec();

    let (synonym_type, synonym_kind) = application::infer_application_arguments(
        state,
        context,
        (function_type, kind),
        &arguments,
        application::Options::SYNONYM,
    )?;

    Ok((synonym_type, synonym_kind))
}

fn is_recursive_synonym<Q>(
    context: &CheckContext<Q>,
    file_id: FileId,
    type_id: TypeItemId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    let has_recursive_error = |grouped: &lowering::GroupedModule| {
        grouped.cycle_errors.iter().any(|error| {
            if let lowering::LoweringError::RecursiveSynonym(recursive) = error {
                recursive.group.contains(&type_id)
            } else {
                false
            }
        })
    };

    let grouped = if file_id == context.id {
        Arc::clone(&context.grouped)
    } else {
        context.queries.grouped(file_id)?
    };

    Ok(has_recursive_error(&grouped))
}
