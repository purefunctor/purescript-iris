mod collection;
mod context;
mod convert;
mod model;
mod render;

pub use building_types::Severity;
pub use collection::{DiagnosticCollection, ErrorCounts, collect_diagnostics, count_errors};
pub use context::{DiagnosticsContext, ExternalQueries};
pub use convert::ToDiagnostics;
pub use model::{Diagnostic, DiagnosticCode, DiagnosticSection, RelatedSpan, Span};
pub use render::{format_rich_with_path, to_lsp_diagnostic};
