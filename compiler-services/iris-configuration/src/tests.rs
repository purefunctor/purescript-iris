use serde_json::{Value, json};

use super::{Configuration, ConfigurationSettings, Diagnostics};

fn settings(value: Value) -> ConfigurationSettings {
    #[cfg(feature = "schema")]
    {
        let schema = serde_json::to_value(super::schema()).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&value), "schema rejected {value}");
    }
    serde_json::from_value::<Option<ConfigurationSettings>>(value).unwrap().unwrap_or_default()
}

#[test]
fn defaults_match_existing_diagnostic_triggers() {
    let configuration = Configuration::default();
    assert_eq!(
        configuration.diagnostics,
        Diagnostics { on_open: true, on_save: true, on_change: false }
    );
    assert_eq!(
        serde_json::to_value(&configuration).unwrap(),
        json!({
            "diagnostics": {"onOpen": true, "onSave": true, "onChange": false}
        })
    );
}

#[test]
fn layers_override_only_supplied_fields() {
    let startup = settings(json!({
        "diagnostics": {"onOpen": false, "onChange": true}
    }));
    let baseline = startup.apply_to(&Configuration::default());
    let initialization = settings(json!({"diagnostics": {"onSave": false}}));
    let baseline = initialization.apply_to(&baseline);
    let runtime = settings(json!({"diagnostics": {"onOpen": true, "onChange": false}}));
    let effective = runtime.apply_to(&baseline);

    assert_eq!(
        effective.diagnostics,
        Diagnostics { on_open: true, on_save: false, on_change: false }
    );
    assert_eq!(
        baseline.diagnostics,
        Diagnostics { on_open: false, on_save: false, on_change: true }
    );

    let replacement = settings(json!({"diagnostics": {"onSave": true}}));
    assert_eq!(
        replacement.apply_to(&baseline).diagnostics,
        Diagnostics { on_open: false, on_save: true, on_change: true }
    );
}

#[test]
fn missing_and_null_inherit_instead_of_resetting_to_defaults() {
    let baseline = Configuration {
        diagnostics: Diagnostics { on_open: false, on_save: false, on_change: true },
    };
    for value in [
        json!(null),
        json!({}),
        json!({"diagnostics": null}),
        json!({"diagnostics": {}}),
        json!({"diagnostics": {"onOpen": null, "onSave": null, "onChange": null}}),
    ] {
        assert_eq!(settings(value.clone()).apply_to(&baseline), baseline, "{value}");
    }
}

#[test]
fn malformed_settings_are_rejected() {
    #[cfg(feature = "schema")]
    let validator =
        jsonschema::validator_for(&serde_json::to_value(super::schema()).unwrap()).unwrap();

    for value in [
        json!(false),
        json!("settings"),
        json!({"$schema": "https://example.com/schema.json"}),
        json!({"unknown": true}),
        json!({"sources": {"kind": "spago"}}),
        json!({"sources": {"kind": "command", "program": "custom"}}),
        json!({"diagnostics": {"onOpened": true}}),
        json!({"diagnostics": {"onOpen": "false"}}),
    ] {
        assert!(serde_json::from_value::<ConfigurationSettings>(value.clone()).is_err(), "{value}");
        #[cfg(feature = "schema")]
        assert!(!validator.is_valid(&value), "schema accepted {value}");
    }
}

#[test]
fn serialization_preserves_overrides() {
    let value = json!({
        "diagnostics": {"onOpen": false, "onChange": true}
    });
    let configuration = settings(value.clone());
    assert_eq!(serde_json::to_value(&configuration).unwrap(), value);
    assert_eq!(serde_json::to_value(ConfigurationSettings::default()).unwrap(), json!({}));

    let effective = configuration.apply_to(&Configuration::default());
    let exported = serde_json::to_value(&effective).unwrap();
    assert_eq!(settings(exported).apply_to(&Configuration::default()), effective);
}

#[cfg(feature = "schema")]
#[test]
fn schema_excludes_serdes_positional_struct_representation() {
    let validator =
        jsonschema::validator_for(&serde_json::to_value(super::schema()).unwrap()).unwrap();
    for value in [json!([]), json!({"diagnostics": []})] {
        assert!(serde_json::from_value::<ConfigurationSettings>(value.clone()).is_ok());
        assert!(!validator.is_valid(&value), "{value}");
    }
}

#[cfg(feature = "schema")]
#[test]
fn checked_in_schema_matches_generated_schema() {
    let generated = serde_json::to_string_pretty(&super::schema()).unwrap();
    assert_eq!(
        include_str!("../configuration.schema.json"),
        format!("{generated}\n"),
        "run just configuration-schema"
    );
}
