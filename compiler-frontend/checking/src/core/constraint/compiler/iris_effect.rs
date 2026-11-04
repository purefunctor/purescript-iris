use building_types::QueryResult;
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::matching::{self, MatchType};
use crate::core::{Type, TypeId, normalise};
use crate::error::ErrorKind;
use crate::state::CheckState;

use super::{CompilerMatch, CompilerResolution, extract_row};

pub enum ServiceSelection {
    Unique(SmolStr),
    Missing,
    Ambiguous(Vec<SmolStr>),
    Stuck { stuck: Vec<u32>, skolem: bool },
}

pub fn select_service<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    service: TypeId,
    services: TypeId,
) -> QueryResult<Option<ServiceSelection>>
where
    Q: ExternalQueries,
{
    let Some(row) = extract_row(state, context, services)? else {
        return Ok(None);
    };

    let mut roles = Vec::new();
    let mut stuck = Vec::new();
    let mut skolem = false;

    for field in row.fields() {
        match matching::types_equal(state, context, service, field.id)? {
            MatchType::Match { .. } => roles.push(SmolStr::clone(&field.label)),
            MatchType::Stuck { stuck: field_stuck, skolem: field_skolem } => {
                stuck.extend(field_stuck);
                skolem |= field_skolem;
            }
            MatchType::Apart => (),
        }
    }

    if roles.len() > 1 {
        return Ok(Some(ServiceSelection::Ambiguous(roles)));
    }

    if let Some(tail) = row.tail() {
        let tail = normalise::expand(state, context, tail)?;
        if let Type::Unification(unification) = *context.lookup_type(tail) {
            stuck.push(unification);
        } else {
            skolem = true;
        }
    }

    if !stuck.is_empty() || skolem {
        return Ok(Some(ServiceSelection::Stuck { stuck, skolem }));
    }

    Ok(Some(match roles.pop() {
        Some(role) => ServiceSelection::Unique(role),
        None => ServiceSelection::Missing,
    }))
}

pub fn match_has<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<CompilerMatch>>
where
    Q: ExternalQueries,
{
    let &[service, services] = arguments else { return Ok(None) };

    let Some(selection) = select_service(state, context, service, services)? else {
        return Ok(None);
    };

    let resolution = match selection {
        ServiceSelection::Unique(_) => CompilerResolution::Synthesized,
        ServiceSelection::Missing => {
            CompilerResolution::Failure(ErrorKind::CapabilityNotDeclared { service, services })
        }
        ServiceSelection::Ambiguous(roles) => {
            let roles = roles.into();
            CompilerResolution::Failure(ErrorKind::AmbiguousCapability { service, services, roles })
        }
        ServiceSelection::Stuck { stuck, skolem } => {
            return Ok(Some(CompilerMatch::Stuck { stuck, skolem }));
        }
    };

    Ok(Some(CompilerMatch::resolved(resolution)))
}
