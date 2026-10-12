use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use foreign_javascript::ForeignQueries;

use crate::selection::{SelectedDiagnostics, select_diagnostics};
use crate::{Diagnostic, DiagnosticsContext, ExternalQueries, ToDiagnostics};

pub struct DiagnosticCollection {
    pub file_id: FileId,
    pub content: Arc<str>,
    diagnostics: Vec<Diagnostic>,
    hidden: Vec<Diagnostic>,
}

impl DiagnosticCollection {
    /// Diagnostics to report, with at most one error for each top-level declaration.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Diagnostics left out of [`DiagnosticCollection::diagnostics`], which
    /// are often caused by a reported error.
    pub fn hidden_diagnostics(&self) -> &[Diagnostic] {
        &self.hidden
    }
}

pub fn collect_diagnostics<Q>(
    queries: &Q,
    source_files: &[FileId],
) -> QueryResult<Vec<DiagnosticCollection>>
where
    Q: ExternalQueries + ForeignQueries + javascript::ModuleQueries,
{
    let collections = source_files.iter().map(|&file_id| {
        let javascript = queries.javascript(file_id)?;
        collect_file_diagnostics(queries, file_id, javascript)
    });
    collections.collect()
}

fn collect_file_diagnostics<Q>(
    queries: &Q,
    file_id: FileId,
    javascript: javascript::ModuleResult<Arc<javascript::Module>>,
) -> QueryResult<DiagnosticCollection>
where
    Q: ExternalQueries + ForeignQueries,
{
    let content = queries.content(file_id)?;
    let (parsed, parse_errors) = queries.parsed(file_id)?;
    let root = parsed.syntax_node();
    let stabilized = queries.stabilized(file_id)?;
    let indexed = queries.indexed(file_id)?;
    let resolved = queries.resolved(file_id)?;
    let lowered = queries.lowered(file_id)?;
    let grouped = queries.grouped(file_id)?;
    let checked = queries.checked(file_id)?;
    let foreign_validation = queries.foreign_validation(file_id)?;

    let context = DiagnosticsContext::new(
        queries,
        &content,
        &root,
        &stabilized,
        &indexed,
        &lowered,
        &checked,
    );

    let mut source = Vec::new();
    for error in &indexed.errors {
        source.extend(error.to_diagnostics(&context));
    }
    for error in parse_errors.iter() {
        source.extend(error.to_diagnostics(&context));
    }
    for error in &lowered.errors {
        source.extend(error.to_diagnostics(&context));
    }
    for error in &grouped.cycle_errors {
        source.extend(error.to_diagnostics(&context));
    }
    for error in &resolved.errors {
        source.extend(error.to_diagnostics(&context));
    }
    for error in &checked.errors {
        source.extend(error.to_diagnostics(&context));
    }

    let mut foreign = Vec::new();
    for error in foreign_validation.errors.iter() {
        foreign.extend(error.to_diagnostics(&context));
    }

    let mut backend = Vec::new();
    match javascript {
        Ok(module) => {
            for diagnostic in module.diagnostics() {
                backend.extend(diagnostic.to_diagnostics(&context));
            }
        }
        Err(error) => backend.extend(error.to_diagnostics(&context)),
    }

    let SelectedDiagnostics { reported, hidden } =
        select_diagnostics(&context, source, foreign, backend);

    Ok(DiagnosticCollection { file_id, content, diagnostics: reported, hidden })
}
