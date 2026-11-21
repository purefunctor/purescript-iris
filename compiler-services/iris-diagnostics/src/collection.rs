use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use foreign_javascript::ForeignQueries;

use crate::{Diagnostic, DiagnosticsContext, ExternalQueries, Severity, ToDiagnostics};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ErrorCounts {
    pub parse: usize,
    pub indexing: usize,
    pub resolving: usize,
    pub lowering: usize,
    pub cycle: usize,
    pub checking: usize,
    pub foreign: usize,
    pub codegen: usize,
}

impl ErrorCounts {
    pub fn has_frontend_errors(self) -> bool {
        self.parse > 0
            || self.indexing > 0
            || self.resolving > 0
            || self.lowering > 0
            || self.cycle > 0
            || self.checking > 0
    }

    pub fn is_empty(self) -> bool {
        !self.has_frontend_errors() && self.foreign == 0 && self.codegen == 0
    }

    pub fn summary(self) -> Option<String> {
        let stages = [
            ("parse", self.parse),
            ("indexing", self.indexing),
            ("resolving", self.resolving),
            ("lowering", self.lowering),
            ("cycle", self.cycle),
            ("checking", self.checking),
            ("foreign", self.foreign),
            ("codegen", self.codegen),
        ];
        let parts = stages.into_iter().filter(|&(_, count)| count > 0).map(|(stage, count)| {
            let noun = if count == 1 { "error" } else { "errors" };
            format!("{count} {stage} {noun}")
        });
        let parts = parts.collect::<Vec<_>>();
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

pub fn count_errors<Q>(queries: &Q, file_id: FileId) -> QueryResult<ErrorCounts>
where
    Q: ExternalQueries + ForeignQueries + javascript::ModuleQueries,
{
    let (_, parse_errors) = queries.parsed(file_id)?;
    let indexed = queries.indexed(file_id)?;
    let resolved = queries.resolved(file_id)?;
    let lowered = queries.lowered(file_id)?;
    let grouped = queries.grouped(file_id)?;
    let checked = queries.checked(file_id)?;
    let foreign_validation = queries.foreign_validation(file_id)?;

    let mut counts = ErrorCounts {
        parse: parse_errors.iter().filter(|error| error.severity().is_error()).count(),
        indexing: indexed.errors.iter().filter(|error| error.severity().is_error()).count(),
        resolving: resolved.errors.iter().filter(|error| error.severity().is_error()).count(),
        lowering: lowered.errors.iter().filter(|error| error.severity().is_error()).count(),
        cycle: grouped.cycle_errors.iter().filter(|error| error.severity().is_error()).count(),
        checking: checked.errors.iter().filter(|error| error.severity().is_error()).count(),
        foreign: foreign_validation
            .errors
            .iter()
            .filter(|error| error.severity().is_error())
            .count(),
        codegen: 0,
    };

    if !counts.has_frontend_errors() {
        counts.codegen = match queries.javascript(file_id)? {
            Ok(module) => module
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.severity().is_error())
                .count(),
            Err(error) => usize::from(error.severity().is_some_and(Severity::is_error)),
        };
    }

    Ok(counts)
}

pub struct DiagnosticCollection {
    pub file_id: FileId,
    pub content: Arc<str>,
    diagnostics: Vec<Diagnostic>,
    foreign_start: usize,
    backend_start: usize,
}

impl DiagnosticCollection {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
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

    let mut diagnostics = Vec::new();
    for error in &indexed.errors {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }
    for error in parse_errors.iter() {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }
    for error in &lowered.errors {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }
    for error in &grouped.cycle_errors {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }
    for error in &resolved.errors {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }
    for error in &checked.errors {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }

    let foreign_start = diagnostics.len();
    for error in foreign_validation.errors.iter() {
        diagnostics.extend(error.to_diagnostics(&context)?);
    }

    let backend_start = diagnostics.len();
    match javascript {
        Ok(module) => {
            for diagnostic in module.diagnostics() {
                diagnostics.extend(diagnostic.to_diagnostics(&context)?);
            }
        }
        Err(error) => diagnostics.extend(error.to_diagnostics(&context)?),
    }

    Ok(DiagnosticCollection { file_id, content, diagnostics, foreign_start, backend_start })
}
