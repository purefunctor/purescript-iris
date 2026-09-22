use files::FileId;
use line_index::LineIndex;
use lsp_types::{Diagnostic, Url};

use crate::{AnalyzerContext, AnalyzerError, AnalyzerHost, common};

pub struct CollectedDiagnostics {
    pub uri: Url,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn implementation<Host>(
    context: &AnalyzerContext<Host>,
    file_id: FileId,
) -> Result<CollectedDiagnostics, AnalyzerError>
where
    Host: AnalyzerHost,
    Host::Queries: diagnostics::ExternalQueries
        + foreign_javascript::ForeignQueries
        + javascript::ModuleQueries,
{
    let queries = context.queries();
    let mut collected = diagnostics::collect_diagnostics(queries, &[file_id])?;
    let collected = collected.pop().expect("one source file should produce one collection");
    let uri = common::file_uri(context, file_id)?;
    let position_encoding = context.position_encoding().into();
    let line_index = LineIndex::new(&collected.content);
    let diagnostics = collected.diagnostics().iter().filter_map(|diagnostic| {
        diagnostics::to_lsp_diagnostic(diagnostic, &line_index, &uri, &position_encoding)
    });

    let diagnostics = diagnostics.collect();
    Ok(CollectedDiagnostics { uri, diagnostics })
}
