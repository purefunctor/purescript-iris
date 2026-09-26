//! Capability registration, `workspace/configuration` requests, and the settings generation
//! counter.

use std::time::Duration;

use lsp_server::ResponseError;
use lsp_types::{
    ClientCapabilities, ConfigurationItem, ConfigurationParams, DidChangeConfigurationNotification,
    DidChangeWatchedFilesNotification, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher,
    GlobPattern, Notification, RegistrationParams, Uri, WorkspaceFolder,
};
use serde_json::Value;

use crate::outgoing::{Outcome, Registration};
use crate::service::SettingsResponse;

/// How long the editor may take to answer `workspace/configuration`.
pub(crate) const CONFIGURATION_DEADLINE: Duration = Duration::from_secs(10);

/// What `iris-lsp-server` negotiated about settings from the `initialize` parameters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Settings {
    capabilities: SettingsCapabilities,
    /// The first workspace folder, which scopes `workspace/configuration`.
    scope: Option<Uri>,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SettingsCapabilities {
    pub(crate) workspace_configuration: bool,
    pub(crate) configuration_registration: bool,
    pub(crate) watched_files_registration: bool,
}

impl SettingsCapabilities {
    pub(crate) fn negotiate(capabilities: &ClientCapabilities) -> SettingsCapabilities {
        let workspace = capabilities.workspace.as_ref();
        let workspace_configuration =
            workspace.is_some_and(|workspace| workspace.configuration == Some(true));
        let configuration_registration = workspace_configuration
            && workspace
                .and_then(|workspace| workspace.did_change_configuration.as_ref())
                .is_some_and(|capability| capability.dynamic_registration == Some(true));
        let watched_files_registration = workspace
            .and_then(|workspace| workspace.did_change_watched_files.as_ref())
            .and_then(|watched_files| watched_files.dynamic_registration)
            .unwrap_or(false);
        SettingsCapabilities {
            workspace_configuration,
            configuration_registration,
            watched_files_registration,
        }
    }
}

impl Settings {
    pub(crate) fn new(
        capabilities: &ClientCapabilities,
        workspace_folders: Option<&[WorkspaceFolder]>,
    ) -> Settings {
        let scope = workspace_folders
            .and_then(|folders| folders.first())
            .map(|folder| Uri::clone(&folder.uri));
        Settings {
            capabilities: SettingsCapabilities::negotiate(capabilities),
            scope,
            generation: 0,
        }
    }

    /// The dynamic registrations to request on `initialized`, in order.
    pub(crate) fn registrations(&self) -> Vec<(Registration, RegistrationParams)> {
        let mut registrations = Vec::new();
        if self.capabilities.watched_files_registration {
            registrations.push((Registration::WatchedFiles, watched_files_registration()));
        }
        if self.capabilities.configuration_registration {
            let registration = lsp_types::Registration {
                id: "iris-workspace-configuration".to_string(),
                method: DidChangeConfigurationNotification::METHOD.to_string(),
                register_options: None,
            };
            let parameters = RegistrationParams { registrations: vec![registration] };
            registrations.push((Registration::ConfigurationChanges, parameters));
        }
        registrations
    }

    pub(crate) fn supports_workspace_configuration(&self) -> bool {
        self.capabilities.workspace_configuration
    }

    /// Starts a new settings generation and returns the `workspace/configuration` parameters
    /// for it.
    pub(crate) fn next_request(&mut self) -> (u64, ConfigurationParams) {
        self.generation = self.generation.wrapping_add(1);
        let parameters = ConfigurationParams {
            items: vec![ConfigurationItem {
                scope_uri: self.scope.clone(),
                section: Some("iris.server".to_string()),
            }],
        };
        (self.generation, parameters)
    }

    /// Turns the outcome of the request for `generation` into a message for the workspace actor,
    /// or `None` if a newer request superseded it.
    pub(crate) fn accept(&self, generation: u64, outcome: Outcome) -> Option<SettingsResponse> {
        if generation != self.generation {
            return None;
        }
        Some(match outcome {
            Outcome::Response(Ok(result)) => SettingsResponse::Received(result),
            Outcome::Response(Err(error)) => SettingsResponse::Failed(describe(&error)),
            Outcome::Expired => {
                SettingsResponse::Failed("workspace/configuration request timed out".to_string())
            }
        })
    }
}

/// Describes an error response the way the settings error message has always shown it.
fn describe(error: &ResponseError) -> String {
    format!("{} (jsonrpc error {})", error.message, error.code)
}

fn watched_files_registration() -> RegistrationParams {
    let watcher = |glob: &str| FileSystemWatcher {
        glob_pattern: GlobPattern::Pattern(glob.to_string()),
        kind: None,
    };
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: vec![watcher("**/*.purs"), watcher("**/*.js"), watcher("**/*.jsx")],
    };
    let register_options = serde_json::to_value(options)
        .expect("invariant violated: watched file registration options must serialize");
    let registration = lsp_types::Registration {
        id: "purescript-source-files".to_string(),
        method: DidChangeWatchedFilesNotification::METHOD.to_string(),
        register_options: Some(register_options),
    };
    RegistrationParams { registrations: vec![registration] }
}

pub(crate) fn to_value(parameters: impl serde::Serialize) -> Value {
    serde_json::to_value(parameters)
        .expect("invariant violated: protocol parameters must serialize")
}

#[cfg(test)]
mod tests {
    use lsp_types::{DidChangeConfigurationClientCapabilities, WorkspaceClientCapabilities};

    use super::*;

    fn capabilities(configuration: Option<bool>, registration: Option<bool>) -> ClientCapabilities {
        ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                configuration,
                did_change_configuration: Some(DidChangeConfigurationClientCapabilities {
                    dynamic_registration: registration,
                }),
                ..WorkspaceClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        }
    }

    #[test]
    fn configuration_registration_requires_workspace_configuration() {
        let negotiated = SettingsCapabilities::negotiate(&capabilities(Some(true), Some(true)));
        assert!(negotiated.workspace_configuration);
        assert!(negotiated.configuration_registration);

        let negotiated = SettingsCapabilities::negotiate(&capabilities(Some(false), Some(true)));
        assert_eq!(negotiated, SettingsCapabilities::default());
    }

    #[test]
    fn only_the_current_generation_is_accepted() {
        let mut settings = Settings::new(&capabilities(Some(true), None), None);
        let (first, _) = settings.next_request();
        let (second, _) = settings.next_request();

        assert_eq!(settings.accept(first, Outcome::Response(Ok(Value::Null))), None);
        assert_eq!(
            settings.accept(second, Outcome::Expired),
            Some(SettingsResponse::Failed("workspace/configuration request timed out".into()))
        );
    }
}
