use building::QueryError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("QueryError: {0}")]
    QueryError(#[from] QueryError),
    #[error("TypeScript export error: {0}")]
    TypeScriptExportError(#[from] ts_rs::ExportError),
}
