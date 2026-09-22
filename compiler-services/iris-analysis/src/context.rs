use std::sync::Arc;

use building_types::QueryProxy;
use checking::core::pretty::PrettyQueries;
use files::FileId;
use lsp_types::Url;

use crate::position::PositionEncoding;

pub trait AnalyzerQueries:
    PrettyQueries
    + QueryProxy<
        Parsed = parsing::FullParsedModule,
        Stabilized = Arc<stabilizing::StabilizedModule>,
        Indexed = Arc<indexing::IndexedModule>,
        Lowered = Arc<lowering::LoweredModule>,
        Resolved = Arc<resolving::ResolvedModule>,
        Checked = Arc<checking::CheckedModule>,
    >
{
}

impl<Queries> AnalyzerQueries for Queries where
    Queries: PrettyQueries
        + QueryProxy<
            Parsed = parsing::FullParsedModule,
            Stabilized = Arc<stabilizing::StabilizedModule>,
            Indexed = Arc<indexing::IndexedModule>,
            Lowered = Arc<lowering::LoweredModule>,
            Resolved = Arc<resolving::ResolvedModule>,
            Checked = Arc<checking::CheckedModule>,
        >
{
}

pub trait AnalyzerHost {
    type Queries: AnalyzerQueries;

    fn queries(&self) -> &Self::Queries;
    fn file_id(&self, uri: &str) -> Option<FileId>;
    fn file_uri(&self, file_id: FileId) -> Result<Option<Url>, url::ParseError>;
    fn active_files(&self) -> impl Iterator<Item = FileId> + '_;
    fn is_editable(&self, file_id: FileId) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyzerCapabilities {
    change_annotations: bool,
}

impl AnalyzerCapabilities {
    pub fn with_change_annotations(mut self) -> AnalyzerCapabilities {
        self.change_annotations = true;
        self
    }

    pub fn has_change_annotations(self) -> bool {
        self.change_annotations
    }
}

pub struct AnalyzerContext<'a, Host> {
    host: &'a Host,
    position_encoding: PositionEncoding,
    capabilities: AnalyzerCapabilities,
}

impl<'a, Host: AnalyzerHost> AnalyzerContext<'a, Host> {
    pub fn new(
        host: &'a Host,
        position_encoding: PositionEncoding,
        capabilities: AnalyzerCapabilities,
    ) -> AnalyzerContext<'a, Host> {
        AnalyzerContext { host, position_encoding, capabilities }
    }

    pub fn queries(&self) -> &Host::Queries {
        self.host.queries()
    }

    pub fn file_id(&self, uri: &str) -> Option<FileId> {
        self.host.file_id(uri)
    }

    pub fn file_uri(&self, file_id: FileId) -> Result<Option<Url>, url::ParseError> {
        self.host.file_uri(file_id)
    }

    pub fn active_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.host.active_files()
    }

    pub fn is_editable(&self, file_id: FileId) -> bool {
        self.host.is_editable(file_id)
    }

    pub fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub fn capabilities(&self) -> AnalyzerCapabilities {
        self.capabilities
    }
}
