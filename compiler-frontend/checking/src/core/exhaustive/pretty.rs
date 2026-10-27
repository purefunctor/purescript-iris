use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;
use smol_str::{SmolStr, SmolStrBuilder, format_smolstr};

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::exhaustive::{PatternConstructor, PatternId, PatternKind};
use crate::state::CheckState;

pub fn pretty_witness<Q>(
    context: &CheckContext<Q>,
    state: &CheckState,
    witness: &[PatternId],
) -> QueryResult<SmolStr>
where
    Q: ExternalQueries,
{
    let patterns = witness.iter().map(|&id| pretty_pattern(context, state, id));
    let patterns = patterns.collect::<QueryResult<Vec<_>>>()?;
    Ok(join_smolstr(&patterns, ", "))
}

fn pretty_pattern<Q>(
    context: &CheckContext<Q>,
    state: &CheckState,
    id: PatternId,
) -> QueryResult<SmolStr>
where
    Q: ExternalQueries,
{
    let pattern = &state.patterns[id];
    match &pattern.kind {
        PatternKind::Wildcard => Ok(SmolStr::new_inline("_")),
        PatternKind::Constructor { constructor } => pretty_constructor(context, state, constructor),
    }
}

fn join_smolstr(items: &[SmolStr], separator: &str) -> SmolStr {
    let mut builder = SmolStrBuilder::default();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            builder.push_str(separator);
        }
        builder.push_str(item);
    }
    builder.finish()
}

fn pretty_constructor<Q>(
    context: &CheckContext<Q>,
    state: &CheckState,
    constructor: &PatternConstructor,
) -> QueryResult<SmolStr>
where
    Q: ExternalQueries,
{
    match constructor {
        PatternConstructor::DataConstructor { file_id, item_id, fields } => {
            let name = lookup_constructor_name(context, *file_id, *item_id)?
                .unwrap_or_else(|| SmolStr::new_inline("<Unknown>"));

            if fields.is_empty() {
                return Ok(name);
            }

            let mut builder = SmolStrBuilder::default();
            builder.push_str(&name);

            for &id in fields.iter() {
                builder.push(' ');
                let rendered = pretty_pattern(context, state, id)?;
                let pattern = &state.patterns[id];
                if let PatternKind::Constructor { constructor } = &pattern.kind
                    && !constructor.fields().is_empty()
                {
                    builder.push('(');
                    builder.push_str(&rendered);
                    builder.push(')');
                } else {
                    builder.push_str(&rendered);
                }
            }

            Ok(builder.finish())
        }
        PatternConstructor::Record { labels, fields } => {
            if labels.len() != fields.len() {
                return Ok(SmolStr::new_inline("{ <invalid> }"));
            }

            let fields = labels.iter().zip(fields.iter()).map(|(label, field_id)| {
                let field = pretty_pattern(context, state, *field_id)?;
                Ok(format_smolstr!("{label}: {field}"))
            });
            let fields = fields.collect::<QueryResult<Vec<_>>>()?;
            let fields = join_smolstr(&fields, ", ");
            Ok(format_smolstr!("{{ {fields} }}"))
        }
        PatternConstructor::Array { fields } => {
            let fields = fields.iter().map(|&id| pretty_pattern(context, state, id));
            let fields = fields.collect::<QueryResult<Vec<_>>>()?;
            let fields = join_smolstr(&fields, ", ");
            Ok(format_smolstr!("[{fields}]"))
        }
        PatternConstructor::Boolean(b) => Ok(SmolStr::from(b.to_string())),
        PatternConstructor::Char(c) => {
            let mut builder = SmolStrBuilder::default();
            builder.push('\'');
            builder.push(*c);
            builder.push('\'');
            Ok(builder.finish())
        }
        PatternConstructor::String(s) => Ok(lowering::literal::encode_normal_string(s).into()),
        PatternConstructor::Integer(i) => Ok(SmolStr::from(i.to_string())),
        PatternConstructor::Number(negative, n) => {
            let mut builder = SmolStrBuilder::default();
            if *negative {
                builder.push('-');
            }
            builder.push_str(n.as_ref());
            Ok(builder.finish())
        }
    }
}

fn lookup_constructor_name<Q>(
    context: &CheckContext<Q>,
    file_id: FileId,
    term_id: TermItemId,
) -> QueryResult<Option<SmolStr>>
where
    Q: ExternalQueries,
{
    let indexed = if file_id == context.id {
        Arc::clone(&context.indexed)
    } else {
        context.queries.indexed(file_id)?
    };

    let item = &indexed.items[term_id];
    Ok(item.name.clone())
}
