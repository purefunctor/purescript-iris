use std::io;

use analyzer::AnalyzerError;
use async_lsp::ErrorCode;
use building::QueryError;
use iris_build::compile::CompileError;
use iris_build::{PackagesError, WorkspaceError};
use iris_spago::SpagoError;
use lsp_types::Url;
use thiserror::Error;
use tokio::task;

#[derive(Error, Debug)]
pub enum LspError {
    #[error("AnalyzerError: {0}")]
    AnalyzerError(#[from] AnalyzerError),
    #[error("QueryError: {0}")]
    QueryError(#[from] QueryError),
    #[error("CompileError: {0}")]
    CompileError(#[from] CompileError),
    #[error("Expected a file URI, received {0}")]
    InvalidFileUri(Url),
    #[error("Expected a PureScript or JavaScript document URI, received {0}")]
    UnsupportedDocumentUri(Url),
    #[error("Invalid content change for document {0}")]
    InvalidContentChange(Url),
    #[error("UrlParseError: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Invalid or missing workspace root")]
    MissingRoot,
    #[error("The Iris workspace is not ready")]
    WorkspaceNotReady,
    #[error("The Iris workspace is already ready")]
    WorkspaceAlreadyReady,
    #[error("The Iris workspace could not be prepared")]
    WorkspaceFailed,
    #[error("WorkspaceError: {0}")]
    WorkspaceError(#[from] WorkspaceError),
    #[error("PackagesError: {0}")]
    PackagesError(#[from] PackagesError),
    #[error("SpagoError: {0}")]
    SpagoError(#[from] SpagoError),
    #[error("IoError: {0}")]
    IoError(#[from] io::Error),
    #[error("JoinError: {0}")]
    JoinError(#[from] task::JoinError),
    #[error("async_lsp::Error: {0}")]
    AsyncLsp(#[from] async_lsp::Error),
}

impl LspError {
    #[inline]
    fn as_query_error(&self) -> Option<&QueryError> {
        match self {
            LspError::AnalyzerError(AnalyzerError::QueryError(query_error)) => Some(query_error),
            LspError::QueryError(query_error) => Some(query_error),
            _ => None,
        }
    }

    pub fn code(&self) -> ErrorCode {
        if matches!(self, LspError::WorkspaceNotReady) {
            return ErrorCode::REQUEST_CANCELLED;
        }
        if let Some(QueryError::Cancelled) = self.as_query_error() {
            return ErrorCode::REQUEST_CANCELLED;
        }
        if matches!(self, LspError::AnalyzerError(AnalyzerError::RenameRejected(_))) {
            return ErrorCode::INVALID_PARAMS;
        }
        ErrorCode::REQUEST_FAILED
    }

    pub fn message(&self) -> &str {
        if matches!(self, LspError::WorkspaceNotReady) {
            return "Workspace is loading";
        }
        if matches!(self, LspError::WorkspaceFailed) {
            return "Workspace preparation failed";
        }
        if let Some(QueryError::Cancelled) = self.as_query_error() {
            return "Request cancelled";
        }
        if let LspError::AnalyzerError(AnalyzerError::RenameRejected(message)) = self {
            return message;
        }
        "Request failed"
    }

    pub fn emit_trace(&self) {
        if let Some(QueryError::Cancelled) = self.as_query_error() {
            tracing::warn!("{self}")
        } else if matches!(self, LspError::AnalyzerError(AnalyzerError::RenameRejected(_))) {
            tracing::warn!("{self}")
        } else {
            tracing::error!("{self}")
        }
    }
}

pub trait AnalyzerResultExt<T> {
    /// Convenience method for handling an [`AnalyzerError::NonFatal`]
    /// error, turning it into [`Result::Ok`] with the given item.
    fn on_non_fatal(self, item: T) -> Result<T, LspError>;
}

impl<T> AnalyzerResultExt<T> for Result<T, AnalyzerError> {
    fn on_non_fatal(self, item: T) -> Result<T, LspError> {
        self.or_else(|error| match error {
            AnalyzerError::NonFatal => Ok(item),
            _ => Err(LspError::from(error)),
        })
    }
}
