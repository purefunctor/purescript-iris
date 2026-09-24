use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use foreign_javascript::ForeignQueries;

use crate::{Diagnostic, DiagnosticsContext, ExternalQueries, ToDiagnostics};

pub struct DiagnosticCollection {
    pub file_id: FileId,
    pub content: Arc<str>,
    diagnostics: Vec<Diagnostic>,
    source_start: usize,
    foreign_start: usize,
    backend_start: usize,
}

impl DiagnosticCollection {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics[self.source_start..]
    }

    pub fn checking_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics[..self.foreign_start]
    }

    pub fn foreign_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics[self.foreign_start..self.backend_start]
    }

    pub fn backend_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics[self.backend_start..]
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
    let (parsed, _) = queries.parsed(file_id)?;
    let root = parsed.syntax_node();
    let stabilized = queries.stabilized(file_id)?;
    let indexed = queries.indexed(file_id)?;
    let resolved = queries.resolved(file_id)?;
    let lowered = queries.lowered(file_id)?;
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

    let mut diagnostics = vec![];
    for error in &indexed.errors {
        diagnostics.extend(error.to_diagnostics(&context));
    }

    let source_start = diagnostics.len();
    for error in &lowered.errors {
        diagnostics.extend(error.to_diagnostics(&context));
    }
    for error in &resolved.errors {
        diagnostics.extend(error.to_diagnostics(&context));
    }
    for error in &checked.errors {
        diagnostics.extend(error.to_diagnostics(&context));
    }

    let foreign_start = diagnostics.len();
    for error in foreign_validation.errors.iter() {
        diagnostics.extend(error.to_diagnostics(&context));
    }

    let backend_start = diagnostics.len();
    match javascript {
        Ok(module) => {
            for diagnostic in module.diagnostics() {
                diagnostics.extend(diagnostic.to_diagnostics(&context));
            }
        }
        Err(error) => diagnostics.extend(error.to_diagnostics(&context)),
    }

    Ok(DiagnosticCollection {
        file_id,
        content,
        diagnostics,
        source_start,
        foreign_start,
        backend_start,
    })
}
