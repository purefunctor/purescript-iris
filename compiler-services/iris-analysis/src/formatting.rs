use building_types::QueryProxy;
use lsp_types::{TextEdit, Uri};
use syntax::{TextRange, TextSize};

use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError, AnalyzerHost};

pub use formatting::{Config, FormatError};

pub fn implementation(
    context: &AnalyzerContext<impl AnalyzerHost>,
    uri: Uri,
    config: &Config,
) -> Result<Option<Vec<TextEdit>>, AnalyzerError> {
    let file_id = context.file_id(uri.as_str()).ok_or(AnalyzerError::NonFatal)?;
    if !context.is_editable(file_id) {
        return Ok(None);
    }
    let content = context.queries().content(file_id)?;
    let formatted = match formatting::format_with_config(&content, config) {
        Ok(formatted) => formatted,
        Err(formatting::FormatError::InvalidSource(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if formatted == content.as_ref() {
        return Ok(Some(Vec::new()));
    }
    let positions = PositionConverter::new(&content, context.position_encoding());
    let length = TextSize::of(content.as_ref());
    let range = positions
        .text_range_to_protocol(TextRange::new(TextSize::new(0), length))
        .ok_or(AnalyzerError::NonFatal)?;
    Ok(Some(vec![TextEdit { range, new_text: formatted }]))
}
