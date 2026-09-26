use building_types::QueryResult;
use lowering::{StringKind, StringLiteral};
use smol_str::{SmolStr, format_smolstr};

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::constraint::matching;
use crate::core::pretty::Pretty;
use crate::core::{Type, TypeId, normalise, toolkit, zonk};
use crate::state::CheckState;

use super::{CompilerMatch, CompilerResolution};

fn first_blocking_unification<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<u32>>
where
    Q: ExternalQueries,
{
    let blocking = matching::collect_blocking(state, context, &[id])?;
    Ok(blocking.into_iter().next())
}

fn extract_symbol_text<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<SmolStr>>
where
    Q: ExternalQueries,
{
    let id = normalise::expand(state, context, id)?;
    if let Type::String(_, value) = context.lookup_type(id) {
        let text = value
            .to_utf8()
            .unwrap_or_else(|_| lowering::literal::encode_normal_string_content(value));
        Ok(Some(SmolStr::from(text)))
    } else {
        Ok(None)
    }
}

fn extract_symbol_with_kind<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    id: TypeId,
) -> QueryResult<Option<(StringKind, lowering::StringLiteral)>>
where
    Q: ExternalQueries,
{
    let id = normalise::expand(state, context, id)?;
    if let Type::String(kind, value) = context.lookup_type(id) {
        Ok(Some((*kind, StringLiteral::clone(value))))
    } else {
        Ok(None)
    }
}

fn is_valid_label(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
}

fn render_label(kind: StringKind, value: &lowering::StringLiteral) -> SmolStr {
    if let Ok(text) = value.to_utf8()
        && is_valid_label(&text)
    {
        return SmolStr::from(text);
    }

    match kind {
        StringKind::String => lowering::literal::encode_normal_string(value).into(),
        StringKind::RawString => {
            let text = value.to_utf8().unwrap_or_else(|_| {
                unreachable!("invariant violated: raw string contains a lone surrogate")
            });
            SmolStr::new(format!(r#""""{text}""""#))
        }
    }
}

/// Renders a `Doc` type into a string for custom type error messages.
///
/// Returns `Ok(Some(text))` on success, `Ok(None)` if the doc structure is
/// unrecognised, and `Err(id)` if stuck on an unsolved unification variable
/// with the given ID.
fn render_doc<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    doc: TypeId,
) -> Result<Option<SmolStr>, RenderStuck>
where
    Q: ExternalQueries,
{
    let doc = normalise::expand(state, context, doc)?;

    if let Type::Unification(u) = *context.lookup_type(doc) {
        return Err(RenderStuck::Blocked(u));
    }

    let (constructor, arguments) = toolkit::extract_type_application(state, context, doc)?;
    let prim = &context.prim_type_error;

    if constructor == prim.text {
        let &[symbol] = arguments.as_slice() else { return Ok(None) };
        if let Some(u) = first_blocking_unification(state, context, symbol)? {
            return Err(RenderStuck::Blocked(u));
        }
        Ok(extract_symbol_text(state, context, symbol)?)
    } else if constructor == prim.quote {
        let &[quoted_type] = arguments.as_slice() else { return Ok(None) };
        if let Some(u) = first_blocking_unification(state, context, quoted_type)? {
            return Err(RenderStuck::Blocked(u));
        }
        let quoted_type = zonk::zonk(state, context, quoted_type)?;
        let rendered = Pretty::new(context.queries, &state.checked).render(quoted_type);
        Ok(Some(rendered))
    } else if constructor == prim.quote_label {
        let &[symbol] = arguments.as_slice() else { return Ok(None) };
        if let Some(u) = first_blocking_unification(state, context, symbol)? {
            return Err(RenderStuck::Blocked(u));
        }
        Ok(extract_symbol_with_kind(state, context, symbol)?
            .map(|(kind, value)| render_label(kind, &value)))
    } else if constructor == prim.beside {
        let &[left, right] = arguments.as_slice() else { return Ok(None) };
        let Some(left_rendered) = render_doc(state, context, left)? else {
            return Ok(None);
        };
        let Some(right_rendered) = render_doc(state, context, right)? else {
            return Ok(None);
        };
        Ok(Some(format_smolstr!("{left_rendered}{right_rendered}")))
    } else if constructor == prim.above {
        let &[upper, lower] = arguments.as_slice() else { return Ok(None) };
        let Some(upper_rendered) = render_doc(state, context, upper)? else {
            return Ok(None);
        };
        let Some(lower_rendered) = render_doc(state, context, lower)? else {
            return Ok(None);
        };
        Ok(Some(format_smolstr!("{upper_rendered}\n{lower_rendered}")))
    } else {
        Ok(None)
    }
}

enum RenderStuck {
    Blocked(u32),
    Query(building_types::QueryError),
}

impl From<building_types::QueryError> for RenderStuck {
    fn from(err: building_types::QueryError) -> Self {
        RenderStuck::Query(err)
    }
}

pub fn match_warn<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<CompilerMatch>>
where
    Q: ExternalQueries,
{
    let &[doc] = arguments else { return Ok(None) };

    let message = match render_doc(state, context, doc) {
        Ok(Some(message)) => message,
        Ok(None) => return Ok(Some(CompilerMatch::Stuck { stuck: Vec::new(), skolem: false })),
        Err(RenderStuck::Blocked(u)) => {
            return Ok(Some(CompilerMatch::Stuck { stuck: vec![u], skolem: false }));
        }
        Err(RenderStuck::Query(cycle)) => return Err(cycle),
    };

    let message_id = context.queries.intern_smol_str(message);
    let resolution = CompilerResolution::Warning { message_id };
    Ok(Some(CompilerMatch::resolved(resolution)))
}

pub fn match_fail<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    arguments: &[TypeId],
) -> QueryResult<Option<CompilerMatch>>
where
    Q: ExternalQueries,
{
    let &[doc] = arguments else { return Ok(None) };

    let message = match render_doc(state, context, doc) {
        Ok(Some(message)) => message,
        Ok(None) => return Ok(Some(CompilerMatch::Stuck { stuck: Vec::new(), skolem: false })),
        Err(RenderStuck::Blocked(u)) => {
            return Ok(Some(CompilerMatch::Stuck { stuck: vec![u], skolem: false }));
        }
        Err(RenderStuck::Query(cycle)) => return Err(cycle),
    };

    let message_id = context.queries.intern_smol_str(message);
    let resolution = CompilerResolution::Failure { message_id };
    Ok(Some(CompilerMatch::resolved(resolution)))
}
