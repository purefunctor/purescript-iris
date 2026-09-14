use std::collections::BTreeMap;
use std::sync::Arc;

use analyzer::position::{PositionConverter, PositionEncoding};
use lsp_types::Url;

use crate::{Document, InputFailure};

#[derive(Clone, Debug)]
pub(crate) struct OpenDocument {
    pub(crate) text: Arc<str>,
    pub(crate) version: i32,
    pub(crate) lifetime: u64,
}

#[derive(Default)]
pub(crate) struct Documents {
    pub(crate) open: BTreeMap<Url, OpenDocument>,
    next_lifetime: u64,
}

pub(crate) fn document_path(uri: &Url) -> Result<std::path::PathBuf, InputFailure> {
    iris_build::analysis::document_path(uri)
        .ok_or_else(|| InputFailure::UnsupportedDocument(Url::clone(uri)))
}

impl Documents {
    pub(crate) fn apply(
        &mut self,
        command: Document,
        encoding: PositionEncoding,
    ) -> Result<Url, InputFailure> {
        match command {
            Document::Open { uri, text, version } => {
                document_path(&uri)?;
                if self.open.contains_key(&uri) {
                    return Err(InputFailure::AlreadyOpen(uri));
                }
                self.next_lifetime =
                    self.next_lifetime.checked_add(1).expect("document lifetime overflow");
                self.open.insert(
                    Url::clone(&uri),
                    OpenDocument { text, version, lifetime: self.next_lifetime },
                );
                Ok(uri)
            }
            Document::Change { uri, version, changes } => {
                document_path(&uri)?;
                let document = self
                    .open
                    .get_mut(&uri)
                    .ok_or_else(|| InputFailure::NotOpen(Url::clone(&uri)))?;
                if version <= document.version {
                    return Err(InputFailure::StaleVersion(uri));
                }
                let mut text = document.text.to_string();
                for change in changes {
                    match change.range {
                        None => text = change.text,
                        Some(range) => {
                            let positions = PositionConverter::new(&text, encoding);
                            let offset = |position| {
                                let position = positions.protocol_position_to_utf8(position)?;
                                positions.utf8_position_to_offset(position).map(usize::from)
                            };
                            let start = offset(range.start)
                                .ok_or_else(|| InputFailure::InvalidRange(Url::clone(&uri)))?;
                            let end = offset(range.end)
                                .ok_or_else(|| InputFailure::InvalidRange(Url::clone(&uri)))?;
                            if start > end {
                                return Err(InputFailure::InvalidRange(uri));
                            }
                            text.replace_range(start..end, &change.text);
                        }
                    }
                }
                document.text = text.into();
                document.version = version;
                Ok(uri)
            }
            Document::Close(uri) => {
                document_path(&uri)?;
                self.open.remove(&uri).ok_or_else(|| InputFailure::NotOpen(Url::clone(&uri)))?;
                Ok(uri)
            }
            Document::Save(uri) => {
                document_path(&uri)?;
                Ok(uri)
            }
        }
    }
}
