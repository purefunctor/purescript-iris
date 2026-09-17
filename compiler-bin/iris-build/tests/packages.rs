use std::fs;
use std::path::{Path, PathBuf};

use iris_build::{
    DiscoveredPackage, DiscoveredPackages, PackagesError, Workspace, discover_packages,
};

fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("test file must have a parent")).unwrap();
    fs::write(path, contents).unwrap();
}

fn write_registry(root: &Path, name: &str, version: &str, dependencies: &str, module: &str) {
    let directory = root.join(format!(".spago/p/{name}-{version}"));
    write_file(
        &directory.join("purs.json"),
        &format!(
            r#"{{
  "name": "{name}",
  "version": "{version}",
  "dependencies": {dependencies}
}}"#,
        ),
    );
    write_file(
        &directory.join(format!("src/{module}")),
        r#"module Test where
"#,
    );
}

fn workspace(root: &Path, selected: Option<&str>) -> Workspace {
    Workspace::discover(root, selected).unwrap()
}

fn package<'a>(discovered: &'a DiscoveredPackages, name: &str) -> &'a DiscoveredPackage {
    discovered.packages.iter().find(|package| package.name == name).unwrap()
}

fn relative_files(root: &Path, package: &DiscoveredPackage) -> Vec<String> {
    let mut files = package
        .files
        .iter()
        .map(|path| path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[test]
fn discovers_exact_registry_closure_from_lockfile() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [foo]
workspace: {}
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join("test/Test.Main.purs"),
        r#"module Test.Main where
"#,
    );
    write_registry(root, "foo", "1.0.0", r#"{"bar": ">=1.0.0 <2.0.0"}"#, "Foo.purs");
    write_registry(root, "foo", "2.0.0", r#"{"bar": ">=1.0.0 <2.0.0"}"#, "Foo.purs");
    write_registry(root, "foo-bar", "9.0.0", r#"{}"#, "FooBar.purs");
    write_registry(root, "bar", "1.2.3", r#"{}"#, "Bar.purs");
    write_file(
        &root.join("spago.lock"),
        r#"{
  "packages": {
    "foo": {"type": "registry", "version": "1.0.0"},
    "bar": {"type": "registry", "version": "1.2.3"}
  }
}"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();
    let names = discovered.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, ["application", "bar", "foo"]);
    assert_eq!(
        relative_files(root, package(&discovered, "application")),
        ["src/Main.purs", "test/Test.Main.purs"]
    );
    assert_eq!(
        relative_files(root, package(&discovered, "foo")),
        [".spago/p/foo-1.0.0/src/Foo.purs"]
    );
    assert_eq!(package(&discovered, "foo").roots, [PathBuf::from(".spago/p/foo-1.0.0")]);
    assert_eq!(package(&discovered, "foo").dependencies[0], "bar");
}

#[test]
fn ignores_workspace_test_dependencies_without_a_test_directory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: []
  test:
    main: Test.Main
    dependencies: [spec]
workspace: {}
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();

    assert!(package(&discovered, "application").dependencies.is_empty());
    assert!(discovered.packages.iter().all(|package| package.name != "spec"));
}

#[test]
fn discovers_inline_git_and_local_dependencies() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [fakelib, locallib]
workspace:
  extraPackages:
    fakelib:
      git: https://example.com/fakelib.git
      ref: feature_Name/test:one
      dependencies: [prelude]
    locallib:
      path: vendor/locallib
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/feature_-_name%stest%cone/src/Fakelib.purs"),
        r#"module Fakelib where
"#,
    );
    write_registry(root, "prelude", "6.0.0", r#"{}"#, "Prelude.purs");
    write_file(
        &root.join("vendor/locallib/spago.yaml"),
        r#"package:
  name: locallib
  dependencies: []
"#,
    );
    write_file(
        &root.join("vendor/locallib/src/Locallib.purs"),
        r#"module Locallib where
"#,
    );
    write_file(
        &root.join("spago.lock"),
        r#"{"packages":{"prelude":{"type":"registry","version":"6.0.0"}}}"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();
    assert_eq!(
        relative_files(root, package(&discovered, "fakelib")),
        [".spago/p/fakelib/feature_-_name%stest%cone/src/Fakelib.purs"]
    );
    assert_eq!(package(&discovered, "fakelib").dependencies[0], "prelude");
    assert!(package(&discovered, "locallib").editable);
}

#[test]
fn local_extra_packages_use_only_library_dependencies() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [locallib]
workspace:
  extraPackages:
    locallib:
      path: vendor/locallib
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join("vendor/locallib/spago.yaml"),
        r#"package:
  name: locallib
  dependencies: []
  test:
    main: Test.Main
    dependencies: [spec]
"#,
    );
    write_file(
        &root.join("vendor/locallib/src/Locallib.purs"),
        r#"module Locallib where
"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();

    assert!(package(&discovered, "locallib").dependencies.is_empty());
    assert!(discovered.packages.iter().all(|package| package.name != "spec"));
}

#[test]
fn explicit_empty_git_dependencies_override_the_checkout_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [fakelib]
workspace:
  extraPackages:
    fakelib:
      git: https://example.com/fakelib.git
      ref: abc123
      dependencies: []
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/abc123/spago.yaml"),
        r#"package:
  name: fakelib
  dependencies: [prelude]
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/abc123/src/Fakelib.purs"),
        r#"module Fakelib where
"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();

    assert!(package(&discovered, "fakelib").dependencies.is_empty());
    assert!(discovered.packages.iter().all(|package| package.name != "prelude"));
}

#[test]
fn reads_only_library_dependencies_from_resolved_git_checkout_subdirectory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [fakelib]
workspace:
  extraPackages:
    fakelib:
      git: https://example.com/fakelib.git
      ref: V1.0.0
      subdir: packages/fakelib
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/deadbee/packages/fakelib/spago.yaml"),
        r#"package:
  name: fakelib
  dependencies: [prelude]
  test:
    main: Test.Main
    dependencies: [spec]
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/deadbee/packages/fakelib/src/Fakelib.purs"),
        r#"module Fakelib where
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/_v1.0.0/packages/fakelib/src/Stale.purs"),
        r#"module Stale where
"#,
    );
    write_registry(root, "prelude", "6.0.0", r#"{}"#, "Prelude.purs");
    write_file(
        &root.join("spago.lock"),
        r#"{"packages":{
  "fakelib":{"type":"git","rev":"deadbee"},
  "prelude":{"type":"registry","version":"6.0.0"}
}}"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();
    assert_eq!(package(&discovered, "fakelib").dependencies, ["prelude"]);
    assert!(discovered.packages.iter().all(|package| package.name != "spec"));
    assert_eq!(
        relative_files(root, package(&discovered, "fakelib")),
        [".spago/p/fakelib/deadbee/packages/fakelib/src/Fakelib.purs"]
    );
}

#[test]
fn escapes_git_refs_with_spago_unicode_semantics() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [decimal-ref, derived-case-ref, future-case-ref, uppercase-ref]
workspace:
  extraPackages:
    decimal-ref:
      git: https://example.com/decimal.git
      ref: release²
      dependencies: []
    derived-case-ref:
      git: https://example.com/derived-case.git
      ref: ʰⅠ
      dependencies: []
    future-case-ref:
      git: https://example.com/future-case.git
      ref: 𐕰
      dependencies: []
    uppercase-ref:
      git: https://example.com/uppercase.git
      ref: İ
      dependencies: []
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join(".spago/p/decimal-ref/release%b2/src/Decimal.purs"),
        r#"module Decimal where
"#,
    );
    write_file(
        &root.join(".spago/p/derived-case-ref/%2b0%2160/src/DerivedCase.purs"),
        r#"module DerivedCase where
"#,
    );
    write_file(
        &root.join(".spago/p/future-case-ref/%10570/src/FutureCase.purs"),
        r#"module FutureCase where
"#,
    );
    write_file(
        &root.join(".spago/p/uppercase-ref/_i/src/Uppercase.purs"),
        r#"module Uppercase where
"#,
    );

    let discovered = discover_packages(&workspace(root, None)).unwrap();

    assert_eq!(
        relative_files(root, package(&discovered, "decimal-ref")),
        [".spago/p/decimal-ref/release%b2/src/Decimal.purs"]
    );
    assert_eq!(
        relative_files(root, package(&discovered, "derived-case-ref")),
        [".spago/p/derived-case-ref/%2b0%2160/src/DerivedCase.purs"]
    );
    assert_eq!(
        relative_files(root, package(&discovered, "future-case-ref")),
        [".spago/p/future-case-ref/%10570/src/FutureCase.purs"]
    );
    assert_eq!(
        relative_files(root, package(&discovered, "uppercase-ref")),
        [".spago/p/uppercase-ref/_i/src/Uppercase.purs"]
    );
}

fn git_subdirectory_error(subdir: &str) -> PackagesError {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        &format!(
            r#"package:
  name: application
  dependencies: [fakelib]
workspace:
  extraPackages:
    fakelib:
      git: https://example.com/fakelib.git
      ref: abc123
      subdir: {subdir}
      dependencies: []
"#
        ),
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join(".spago/p/fakelib/abc123/src/Fakelib.purs"),
        r#"module Fakelib where
"#,
    );
    discover_packages(&workspace(root, None)).unwrap_err()
}

#[test]
fn rejects_unsafe_git_subdirectories() {
    for subdir in ["/tmp/fakelib", "../fakelib", "./fakelib"] {
        assert!(matches!(
            git_subdirectory_error(subdir),
            PackagesError::UnsafeGitSubdirectory { .. }
        ));
    }
}

#[cfg(unix)]
#[test]
fn rejects_git_subdirectory_symlink_escape() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [fakelib]
workspace:
  extraPackages:
    fakelib:
      git: https://example.com/fakelib.git
      ref: abc123
      subdir: package
      dependencies: []
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    fs::create_dir_all(root.join(".spago/p/fakelib/abc123")).unwrap();
    symlink(outside.path(), root.join(".spago/p/fakelib/abc123/package")).unwrap();
    let error = discover_packages(&workspace(root, None)).unwrap_err();
    assert!(matches!(error, PackagesError::EscapedGitSubdirectory { .. }));
}

#[test]
fn reports_unfetched_legacy_and_conflicting_packages() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [missing]
workspace: {}
"#,
    );
    write_file(
        &root.join("spago.lock"),
        r#"{"packages":{"missing":{"type":"registry","version":"1.0.0"}}}"#,
    );
    assert!(matches!(
        discover_packages(&workspace(root, None)).unwrap_err(),
        PackagesError::UnfetchedPackage { .. }
    ));

    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [legacy]
workspace:
  extraPackages:
    legacy:
      repo: https://example.com/legacy.git
      version: v1.0.0
"#,
    );
    assert!(matches!(
        discover_packages(&workspace(root, None)).unwrap_err(),
        PackagesError::LegacyPackage { .. }
    ));

    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [selfish]
workspace:
  extraPackages:
    selfish:
      path: .
"#,
    );
    assert!(matches!(
        discover_packages(&workspace(root, None)).unwrap_err(),
        PackagesError::ConflictingSource { .. }
    ));
}

#[test]
fn reports_registry_manifest_identity_mismatch() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: [foo]
workspace: {}
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_registry(root, "foo", "1.0.0", r#"{}"#, "Foo.purs");
    write_file(
        &root.join(".spago/p/foo-1.0.0/purs.json"),
        r#"{"name":"other","version":"1.0.0","dependencies":{}}"#,
    );
    write_file(
        &root.join("spago.lock"),
        r#"{"packages":{"foo":{"type":"registry","version":"1.0.0"}}}"#,
    );
    assert!(matches!(
        discover_packages(&workspace(root, None)).unwrap_err(),
        PackagesError::RegistryIdentity { .. }
    ));
}

#[test]
fn limits_closure_to_selected_workspace_package() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_file(
        &root.join("spago.yaml"),
        r#"package:
  name: application
  dependencies: []
workspace: {}
"#,
    );
    write_file(
        &root.join("src/Main.purs"),
        r#"module Main where
"#,
    );
    write_file(
        &root.join("other/spago.yaml"),
        r#"package:
  name: other
  dependencies: []
"#,
    );
    write_file(
        &root.join("other/src/Other.purs"),
        r#"module Other where
"#,
    );

    let selected = discover_packages(&workspace(root, Some("other"))).unwrap();
    assert_eq!(
        selected.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(),
        ["other"]
    );
    let all = discover_packages(&workspace(root, None)).unwrap();
    assert_eq!(
        all.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(),
        ["application", "other"]
    );
}
