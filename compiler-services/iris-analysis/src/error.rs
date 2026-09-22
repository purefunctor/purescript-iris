pub use building_types::QueryError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AnalyzerError {
    #[error("Non-fatal error")]
    NonFatal,
    #[error("Rename rejected: {0}")]
    RenameRejected(String),
    #[error("QueryError: {0}")]
    QueryError(#[from] QueryError),
    #[error("UrlParseError: {0}")]
    UrlParseError(#[from] url::ParseError),
}
