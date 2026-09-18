use std::path::Path;

use smol_str::SmolStr;

#[test]
fn parses_registry_package_manifest() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/console-purs.json");
    let contents = std::fs::read_to_string(&path).unwrap();
    let manifest = iris_spago::parse_registry_manifest(&contents).unwrap();
    let dependencies = manifest.dependency_names().map(SmolStr::as_str).collect::<Vec<_>>();
    insta::assert_debug_snapshot!((&manifest, dependencies));
}

#[test]
fn requires_registry_package_dependencies() {
    let error =
        iris_spago::parse_registry_manifest(r#"{"name":"leaf","version":"1.0.0"}"#).unwrap_err();

    assert!(error.to_string().contains("missing field `dependencies`"));
}
