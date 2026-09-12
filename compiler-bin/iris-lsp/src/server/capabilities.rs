use analyzer::AnalyzerCapabilities;
use analyzer::position::PositionEncoding;
use lsp_types::{InitializeParams, PositionEncodingKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigurationCapabilities {
    pub workspace_configuration: bool,
    pub dynamic_registration: bool,
}

pub fn negotiate_configuration_capabilities(
    parameters: &InitializeParams,
) -> ConfigurationCapabilities {
    let Some(workspace) = &parameters.capabilities.workspace else {
        return ConfigurationCapabilities::default();
    };
    let workspace_configuration = workspace.configuration == Some(true);
    let dynamic_registration = workspace_configuration
        && workspace
            .did_change_configuration
            .as_ref()
            .is_some_and(|capability| capability.dynamic_registration == Some(true));
    ConfigurationCapabilities { workspace_configuration, dynamic_registration }
}

pub fn negotiate_analyzer_capabilities(parameters: &InitializeParams) -> AnalyzerCapabilities {
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

pub fn negotiate_position_encoding(parameters: &InitializeParams) -> PositionEncoding {
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

#[cfg(test)]
mod tests {
    use lsp_types::{
        ClientCapabilities, DynamicRegistrationClientCapabilities, GeneralClientCapabilities,
        WorkspaceClientCapabilities,
    };

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

    #[test]
    fn workspace_configuration_and_registration_are_negotiated_independently() {
        let parameters = InitializeParams {
            capabilities: ClientCapabilities {
                workspace: Some(WorkspaceClientCapabilities {
                    configuration: Some(true),
                    did_change_configuration: Some(DynamicRegistrationClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    ..WorkspaceClientCapabilities::default()
                }),
                ..ClientCapabilities::default()
            },
            ..InitializeParams::default()
        };

        assert_eq!(
            negotiate_configuration_capabilities(&parameters),
            ConfigurationCapabilities { workspace_configuration: true, dynamic_registration: true }
        );

        let parameters = InitializeParams {
            capabilities: ClientCapabilities {
                workspace: Some(WorkspaceClientCapabilities {
                    configuration: Some(false),
                    did_change_configuration: Some(DynamicRegistrationClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    ..WorkspaceClientCapabilities::default()
                }),
                ..ClientCapabilities::default()
            },
            ..InitializeParams::default()
        };

        assert_eq!(
            negotiate_configuration_capabilities(&parameters),
            ConfigurationCapabilities::default()
        );
    }
}
