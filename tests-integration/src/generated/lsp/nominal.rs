//! `-- nominal <query>` directives, which address items by qualified name through
//! `iris_analysis::nominal`, as `iris watch query` does.

use building::QueryEngine;
use files::Files;
use iris_analysis::nominal::{self, InstanceSearch, NamedItem};
use iris_analysis::position::PositionEncoding;
use iris_analysis::{AnalyzerCapabilities, AnalyzerContext};
use itertools::Itertools;
use lsp_types::Location;

use super::{IntegrationAnalyzerHost, render_location};

pub(super) const DIRECTIVE: &str = "-- nominal ";

type Context<'a> = AnalyzerContext<'a, IntegrationAnalyzerHost<'a>>;

pub(super) fn dispatch(result: &mut String, engine: &QueryEngine, files: &Files, query: &str) {
    let host = IntegrationAnalyzerHost { queries: engine, files };
    let capabilities = AnalyzerCapabilities::default();
    let context = AnalyzerContext::new(&host, PositionEncoding::Utf16, capabilities);
    let words = query.split_whitespace().collect_vec();
    let lines = match words.as_slice() {
        ["signature", name] => signature(&context, name),
        ["references", name] => references(&context, name),
        ["instances", "class", name] => instances(&context, name, InstanceSearch::OfClass),
        ["instances", "type", name] => instances(&context, name, InstanceSearch::MentioningType),
        ["dependents", module] => dependents(&context, files, module),
        ["search", pattern] => search(&context, files, pattern),
        _ => Err(format!("unknown nominal query {query:?}")),
    };
    let text = match lines {
        Ok(lines) if lines.is_empty() => "<empty>".to_string(),
        Ok(lines) => lines.join("\n"),
        Err(message) => format!("<error> {message}"),
    };
    result.push_str(&text);
    result.push('\n');
}

fn items(context: &Context<'_>, name: &str) -> Result<Vec<NamedItem>, String> {
    let engine = context.queries();
    let qualified = nominal::split_qualified_name(engine, name)
        .ok_or_else(|| format!("{name} names no item of a loaded module"))?;
    nominal::lookup(engine, qualified.module_file, qualified.item)
        .map_err(|error| error.to_string())
}

fn signature(context: &Context<'_>, name: &str) -> Result<Vec<String>, String> {
    let engine = context.queries();
    let mut lines = Vec::new();
    for item in items(context, name)? {
        let signature = nominal::signature(engine, item).map_err(|error| error.to_string())?;
        let documentation =
            nominal::documentation(engine, item).map_err(|error| error.to_string())?;
        let definition = nominal::definition(context, item).map_err(|error| error.to_string())?;
        lines.push(signature.unwrap_or_else(|| "<unchecked>".to_string()));
        lines.extend(documentation.map(|documentation| format!("documentation {documentation:?}")));
        lines.push(format!("defined at {}", render_location(definition)));
    }
    Ok(lines)
}

fn references(context: &Context<'_>, name: &str) -> Result<Vec<String>, String> {
    let mut locations = Vec::new();
    for item in items(context, name)? {
        let references = nominal::references(context, item).map_err(|error| error.to_string())?;
        locations.extend(references);
    }
    locations.sort_by_key(location_order);
    Ok(locations.into_iter().map(render_location).collect())
}

/// Orders locations by file, then numerically by position, rather than by their rendered text.
fn location_order(location: &Location) -> (String, u32, u32) {
    let start = location.range.start;
    (location.uri.as_str().to_string(), start.line, start.character)
}

fn instances(
    context: &Context<'_>,
    name: &str,
    search: InstanceSearch,
) -> Result<Vec<String>, String> {
    let mut instances = Vec::new();
    for item in items(context, name)? {
        let NamedItem::Type(file_id, type_id) = item else { continue };
        let found = nominal::instances(context, (file_id, type_id), search);
        instances.extend(found.map_err(|error| error.to_string())?);
    }
    instances.sort_by_key(|instance| location_order(&instance.location));
    let lines = instances.into_iter().map(|instance| {
        let head = instance.head.unwrap_or_else(|| "<unchecked>".to_string());
        format!("{head} {}", render_location(instance.location))
    });
    Ok(lines.collect())
}

fn dependents(context: &Context<'_>, files: &Files, module: &str) -> Result<Vec<String>, String> {
    let engine = context.queries();
    let target =
        engine.module_file(module).ok_or_else(|| format!("no module is named {module}"))?;
    let module_name = |file_id| {
        let name = nominal::module_name(engine, file_id).map_err(|error| error.to_string())?;
        Ok::<_, String>(name.unwrap_or_else(|| "<unnamed>".to_string()))
    };
    let dependents = nominal::dependents(engine, files.iter_id(), target);
    let dependents = dependents.map_err(|error| error.to_string())?;
    let lines = dependents.into_iter().map(|dependent| {
        let name = module_name(dependent.file_id)?;
        match dependent.through {
            Some(through) => Ok(format!("{name} through {}", module_name(through)?)),
            None => Ok(name),
        }
    });
    lines.collect()
}

fn search(context: &Context<'_>, files: &Files, pattern: &str) -> Result<Vec<String>, String> {
    let engine = context.queries();
    let found = nominal::search(engine, files.iter_id(), pattern);
    let found = found.map_err(|error| error.to_string())?;
    let lines = found.into_iter().map(|found| {
        let name = format!("{}.{}", found.module, found.name);
        let signature = nominal::signature_on_one_line(engine, found.item, &name);
        let signature = signature.map_err(|error| error.to_string())?;
        Ok(format!("{} {}", found.rank, signature.unwrap_or_else(|| format!("{name} <unchecked>"))))
    });
    lines.collect()
}
