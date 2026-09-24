//! Position encoding and analyzer capability negotiation, and the server capabilities.

use iris_analysis::AnalyzerCapabilities;
use iris_analysis::position::PositionEncoding;
use lsp_types::*;

pub(crate) fn negotiate_analyzer_capabilities(
    parameters: &InitializeParams,
) -> AnalyzerCapabilities {
    let workspace_edit = parameters
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.workspace_edit.as_ref());
    let honors_rename_annotations = parameters
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.rename.as_ref())
        .is_some_and(|rename| rename.honors_change_annotations == Some(true));
    let change_annotations = workspace_edit.is_some_and(|workspace_edit| {
        workspace_edit.document_changes == Some(true)
            && workspace_edit.change_annotation_support.is_some()
            && honors_rename_annotations
    });

    if change_annotations {
        AnalyzerCapabilities::default().with_change_annotations()
    } else {
        AnalyzerCapabilities::default()
    }
}

pub(crate) fn negotiate_position_encoding(parameters: &InitializeParams) -> PositionEncoding {
    let Some(encodings) = parameters
        .capabilities
        .general
        .as_ref()
        .and_then(|general| general.position_encodings.as_ref())
    else {
        return PositionEncoding::Utf16;
    };

    if encodings.contains(&PositionEncodingKind::UTF8) {
        PositionEncoding::Utf8
    } else if encodings.contains(&PositionEncodingKind::UTF16) {
        PositionEncoding::Utf16
    } else if encodings.contains(&PositionEncodingKind::UTF32) {
        PositionEncoding::Utf32
    } else {
        PositionEncoding::Utf16
    }
}

pub(crate) fn initialize_result(
    name: &str,
    version: &str,
    position_encoding: PositionEncoding,
) -> InitializeResult {
    InitializeResult {
        server_info: Some(ServerInfo {
            name: name.to_string(),
            version: Some(version.to_string()),
        }),
        capabilities: server_capabilities(position_encoding),
    }
}

fn server_capabilities(position_encoding: PositionEncoding) -> ServerCapabilities {
    ServerCapabilities {
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            trigger_characters: Some(vec![".".to_string()]),
            all_commit_characters: None,
            work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
            completion_item: Some(ServerCompletionItemOptions {
                label_details_support: Some(true),
            }),
        }),
        code_action_provider: Some(CodeActionProvider::CodeActionOptions(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QuickFix]),
            ..CodeActionOptions::default()
        })),
        definition_provider: Some(DefinitionProvider::Bool(true)),
        hover_provider: Some(HoverProvider::Bool(true)),
        references_provider: Some(ReferencesProvider::Bool(true)),
        rename_provider: Some(RenameProvider::RenameOptions(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
        })),
        document_highlight_provider: Some(DocumentHighlightProvider::Bool(true)),
        workspace_symbol_provider: Some(WorkspaceSymbolProvider::Bool(true)),
        document_symbol_provider: Some(DocumentSymbolProvider::Bool(true)),
        semantic_tokens_provider: Some(SemanticTokensProvider::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                legend: SemanticTokensLegend {
                    token_types: legend(iris_analysis::semantic_tokens::TOKEN_TYPES),
                    token_modifiers: legend(iris_analysis::semantic_tokens::TOKEN_MODIFIERS),
                },
                range: Some(SemanticTokensOptionsRange::Bool(false)),
                full: Some(Full::Bool(true)),
            },
        )),
        text_document_sync: Some(TextDocumentSync::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::Incremental),
            save: Some(Save::Bool(true)),
            ..TextDocumentSyncOptions::default()
        })),
        position_encoding: Some(PositionEncodingKind::from(position_encoding)),
        ..ServerCapabilities::default()
    }
}

fn legend(names: &[impl ToString]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[cfg(test)]
mod tests {
    use lsp_types::{ClientCapabilities, GeneralClientCapabilities};

    use super::*;

    fn initialize_parameters(
        position_encodings: Option<Vec<PositionEncodingKind>>,
    ) -> InitializeParams {
        InitializeParams {
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings,
                    ..GeneralClientCapabilities::default()
                }),
                ..ClientCapabilities::default()
            },
            ..InitializeParams::default()
        }
    }

    #[test]
    fn defaults_to_utf16_without_client_preference() {
        let parameters = InitializeParams::default();

        let encoding = negotiate_position_encoding(&parameters);
        assert_eq!(encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn prefers_utf8_when_available() {
        let parameters = initialize_parameters(Some(vec![
            PositionEncodingKind::UTF32,
            PositionEncodingKind::UTF16,
            PositionEncodingKind::UTF8,
        ]));

        let encoding = negotiate_position_encoding(&parameters);
        assert_eq!(encoding, PositionEncoding::Utf8);
    }

    #[test]
    fn falls_back_to_utf16_before_utf32() {
        let parameters = initialize_parameters(Some(vec![
            PositionEncodingKind::UTF32,
            PositionEncodingKind::UTF16,
        ]));

        let encoding = negotiate_position_encoding(&parameters);
        assert_eq!(encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn supports_utf32_when_it_is_the_only_known_option() {
        let parameters = initialize_parameters(Some(vec![PositionEncodingKind::UTF32]));

        let encoding = negotiate_position_encoding(&parameters);
        assert_eq!(encoding, PositionEncoding::Utf32);
    }
}
