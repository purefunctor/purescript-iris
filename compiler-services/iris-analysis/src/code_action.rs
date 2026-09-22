mod holes;

use std::collections::HashMap;

use building_types::QueryProxy;
use files::FileId;
use lowering::{ExpressionId, TypeId};
use lsp_types::*;

use crate::position::PositionConverter;
use crate::{AnalyzerContext, AnalyzerError, locate};

pub fn implementation(
    language: &AnalyzerContext<impl crate::AnalyzerHost>,
    uri: Url,
    range: Range,
    action_context: CodeActionContext,
) -> Result<Option<CodeActionResponse>, AnalyzerError> {
    let file = {
        let uri = uri.as_str();
        language.file_id(uri).ok_or(AnalyzerError::NonFatal)?
    };

    let content = language.queries().content(file)?;
    let positions = PositionConverter::new(&content, language.position_encoding());
    let position =
        positions.protocol_position_to_utf8(range.start).ok_or(AnalyzerError::NonFatal)?;

    let located = locate::locate(language.queries(), file, &positions, position)?;
    let kinds = RequestedCodeActionKinds { only: action_context.only.as_deref() };
    let request =
        CodeActionRequest { language, uri: &uri, file, positions: &positions, kinds, located };

    let mut actions = vec![];
    holes::collect(&request, &mut actions)?;

    let has_actions = !actions.is_empty();
    Ok(has_actions.then_some(actions))
}

pub struct CodeActionRequest<'request, 'language, Host> {
    pub language: &'request AnalyzerContext<'language, Host>,
    pub uri: &'request Url,
    pub file: FileId,
    pub positions: &'request PositionConverter<'request>,
    pub kinds: RequestedCodeActionKinds<'request>,
    pub located: locate::Located,
}

#[derive(Clone, Copy)]
pub struct RequestedCodeActionKinds<'a> {
    only: Option<&'a [CodeActionKind]>,
}

impl RequestedCodeActionKinds<'_> {
    pub fn includes(&self, action_kind: &CodeActionKind) -> bool {
        let Some(only) = self.only else { return true };
        only.iter().any(|kind| code_action_kind_matches(kind, action_kind))
    }
}

fn code_action_kind_matches(requested: &CodeActionKind, action_kind: &CodeActionKind) -> bool {
    let requested = requested.as_str();
    let action_kind = action_kind.as_str();

    let Some(suffix) = action_kind.strip_prefix(requested) else { return false };
    suffix.is_empty() || suffix.starts_with('.')
}

pub fn workspace_edit(uri: &Url, edits: Vec<TextEdit>) -> WorkspaceEdit {
    let mut changes = HashMap::default();

    let uri = Url::clone(uri);
    changes.insert(uri, edits);

    WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }
}

pub fn expression_range(
    request: &CodeActionRequest<impl crate::AnalyzerHost>,
    expression_id: ExpressionId,
) -> Result<Range, AnalyzerError> {
    let (parsed, _) = request.language.queries().parsed(request.file)?;
    let stabilized = request.language.queries().stabilized(request.file)?;

    let range = locate::id_range(request.positions, &parsed, &stabilized, expression_id)
        .ok_or(AnalyzerError::NonFatal)?;

    request.positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)
}

pub fn type_range(
    request: &CodeActionRequest<impl crate::AnalyzerHost>,
    type_id: TypeId,
) -> Result<Range, AnalyzerError> {
    let (parsed, _) = request.language.queries().parsed(request.file)?;
    let stabilized = request.language.queries().stabilized(request.file)?;

    let range = locate::id_range(request.positions, &parsed, &stabilized, type_id)
        .ok_or(AnalyzerError::NonFatal)?;

    request.positions.utf8_range_to_protocol(range).ok_or(AnalyzerError::NonFatal)
}
