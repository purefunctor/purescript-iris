use std::path::PathBuf;

use tests_compatibility::registry::RegistryError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifierError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error("failed to convert path to file URL: {0}")]
    FileUrl(PathBuf),
    #[error(
        "unsupported report schema version {found}; latest supported version is {latest_supported}"
    )]
    UnsupportedReportSchemaVersion { found: u32, latest_supported: u32 },
    #[error("cannot compare compatibility reports: {0}")]
    IncompatibleReports(String),
}
