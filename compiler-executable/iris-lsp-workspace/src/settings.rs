//! Validating settings with `iris-configuration`, fallback, and error messages.

use std::sync::Arc;

use iris_configuration::{Configuration, ConfigurationSettings};
use iris_lsp_server::SettingsResponse;
use serde_json::Value;

/// The settings layer the workspace actor resolves editor settings against, and whether any
/// settings were applied yet.
pub(crate) struct Settings {
    pub(crate) default: Arc<Configuration>,
    pub(crate) initialized: bool,
}

/// What a settings response asks the workspace actor to do.
pub(crate) enum SettingsUpdate {
    /// The editor has no settings to offer; use the defaults.
    Defaults,
    /// Settings resolved over the defaults, or why they could not be.
    Resolved(Result<Configuration, String>),
}

impl Settings {
    pub(crate) fn new() -> Settings {
        Settings { default: Arc::new(Configuration::default()), initialized: false }
    }

    pub(crate) fn update(&self, response: SettingsResponse) -> SettingsUpdate {
        match response {
            SettingsResponse::Unsupported => SettingsUpdate::Defaults,
            SettingsResponse::Received(result) => SettingsUpdate::Resolved(self.resolve(result)),
            SettingsResponse::Failed(error) => {
                SettingsUpdate::Resolved(Err(format!("Failed to retrieve Iris settings: {error}")))
            }
        }
    }

    /// Resolves the result of `workspace/configuration` for section `iris.server`.
    fn resolve(&self, result: Value) -> Result<Configuration, String> {
        let mut values = serde_json::from_value::<Vec<Value>>(result).map_err(|error| {
            format!("Failed to retrieve Iris settings: deserialization failed: {error}")
        })?;
        if values.len() != 1 {
            return Err(format!(
                "Invalid workspace/configuration response: expected one item, received {}",
                values.len()
            ));
        }
        let value = values.pop().expect("invariant violated: expected one configuration item");
        serde_json::from_value::<Option<ConfigurationSettings>>(value)
            .map(|settings| settings.unwrap_or_default().apply_to(&self.default))
            .map_err(|error| format!("Invalid Iris settings: {error}"))
    }

    /// The user-visible message for a settings error, which says which settings stay active.
    pub(crate) fn error_message(&self, error: &str) -> String {
        if self.initialized {
            format!("{error}. The previous Iris settings remain active.")
        } else {
            format!("{error}. Iris will use its default settings.")
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn resolved(update: SettingsUpdate) -> Result<Configuration, String> {
        match update {
            SettingsUpdate::Resolved(result) => result,
            SettingsUpdate::Defaults => panic!("expected resolved settings"),
        }
    }

    #[test]
    fn settings_resolve_over_the_defaults() {
        let settings = Settings::new();
        let received = SettingsResponse::Received(json!([{"diagnostics": {"onChange": true}}]));
        let configuration = resolved(settings.update(received)).unwrap();
        assert!(configuration.diagnostics.on_change);
        assert!(configuration.diagnostics.on_open);

        let received = SettingsResponse::Received(json!([null]));
        assert_eq!(resolved(settings.update(received)).unwrap(), Configuration::default());
    }

    #[test]
    fn invalid_settings_explain_the_problem() {
        let settings = Settings::new();
        let cases = [
            (
                SettingsResponse::Received(json!([{"diagnostics": {"onOpen": "invalid"}}])),
                "Invalid Iris settings: invalid type: string \"invalid\", expected a boolean",
            ),
            (
                SettingsResponse::Received(json!([{}, {}])),
                "Invalid workspace/configuration response: expected one item, received 2",
            ),
            (
                SettingsResponse::Received(Value::Null),
                "Failed to retrieve Iris settings: deserialization failed: invalid type: null, expected a sequence",
            ),
            (
                SettingsResponse::Failed("workspace/configuration request timed out".into()),
                "Failed to retrieve Iris settings: workspace/configuration request timed out",
            ),
        ];
        for (response, expected) in cases {
            assert_eq!(resolved(settings.update(response)).unwrap_err(), expected);
        }
    }
}
