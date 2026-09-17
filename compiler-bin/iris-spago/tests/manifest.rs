use std::path::Path;

use smol_str::SmolStr;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap()
}

fn parse_fixture(name: &str) -> iris_spago::Manifest {
    iris_spago::parse_manifest(&fixture(name)).unwrap()
}

#[test]
fn parses_minimal_package() {
    insta::assert_debug_snapshot!(parse_fixture("minimal-package.yaml"));
}

#[test]
fn parses_full_package() {
    insta::assert_debug_snapshot!(parse_fixture("full-package.yaml"));
}

#[test]
fn parses_registry_workspace() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-registry.yaml"));
}

#[test]
fn parses_registry_extra_packages() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-extra-registry.yaml"));
}

#[test]
fn parses_git_extra_packages() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-extra-git.yaml"));
}

#[test]
fn parses_local_extra_packages() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-extra-local.yaml"));
}

#[test]
fn parses_legacy_extra_packages() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-extra-legacy.yaml"));
}

#[test]
fn parses_url_package_set() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-packageset-url.yaml"));
}

#[test]
fn parses_path_package_set() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-packageset-path.yaml"));
}

#[test]
fn parses_unconsumed_future_options() {
    insta::assert_debug_snapshot!(parse_fixture("workspace-future-options.yaml"));
}

#[test]
fn derives_package_metadata_and_source_globs() {
    let manifest = parse_fixture("workspace-extra-git.yaml");
    let package = manifest.package.as_ref().expect("fixture has a package");
    let core = package.core_dependency_names().map(SmolStr::as_str).collect::<Vec<_>>();
    let test = package.test_dependency_names().map(SmolStr::as_str).collect::<Vec<_>>();
    let all = package.all_dependency_names().map(SmolStr::as_str).collect::<Vec<_>>();
    let globs = iris_spago::package_source_directories()
        .iter()
        .map(|directory| iris_spago::source_glob(directory))
        .collect::<Vec<_>>();
    let workspace = manifest.workspace.as_ref().expect("fixture has a workspace");
    let extra = workspace.extra_packages.iter().map(|(name, extra)| {
        let dependencies =
            extra.dependency_names().into_iter().map(SmolStr::as_str).collect::<Vec<_>>();
        (name, extra.subdirectory(), extra.dependency_source_directories(), dependencies)
    });

    let extra = extra.collect::<Vec<_>>();
    insta::assert_debug_snapshot!((core, test, all, globs, extra));
}

#[test]
fn rejects_constrained_test_without_main() {
    let error = iris_spago::parse_manifest(
        "package:\n  name: application\n  dependencies: []\n  test:\n    dependencies: []\n",
    )
    .unwrap_err();
    insta::assert_snapshot!(error.to_string());
}
