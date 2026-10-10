use std::path::Path;
use std::{env, fs};

use super::support::{
    TestWorkspace, assert_success, install_failing_spago_launcher, install_spago_launcher,
};

#[test]
fn creates_a_spago_project_with_the_latest_supported_package_set() {
    let workspace = TestWorkspace::empty();
    let output = workspace.command(&["new", "--name", "example"]);
    assert_success(&output);
    assert!(output.stdout.ends_with(b"\n"));
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stdout), @"
    Created package `example` with package set 81.1.0.
    Run `iris build` to get started.
    ");
    assert!(output.stderr.is_empty());
    workspace
        .assert_spago_calls("", &[&["registry", "package-sets", "--latest", "--json", "--quiet"]]);
    fs::remove_file(workspace.path().join("spago-calls")).unwrap();

    insta::assert_snapshot!(workspace.summary(), @r#"
    --- .gitignore
    .spago/
    output/
    --- spago.yaml
    package:
      name: example
      dependencies:
        - console
        - effect
        - prelude
      test:
        main: Test.Main
        dependencies:
          - assert
    workspace:
      packageSet:
        registry: 81.1.0
    --- src/Main.purs
    module Main where

    import Prelude

    import Effect (Effect)
    import Effect.Console (log)

    main :: Effect Unit
    main = do
      log "🍝"
    --- test/Test/Main.purs
    module Test.Main where

    import Prelude

    import Effect (Effect)
    import Effect.Class.Console (log)

    main :: Effect Unit
    main = do
      log "🍕"
      log "You should add some tests."
    "#);
}

#[test]
fn builds_a_new_project_with_local_spago() {
    let workspace = TestWorkspace::empty();
    let new_output = workspace.command(&["new", "--name", "example"]);
    assert_success(&new_output);

    install_spago_launcher(
        &workspace.path().join("node_modules/.bin"),
        Path::new(env!("CARGO_BIN_EXE_spago-e2e")),
    );
    let global_directory = workspace.path().join("global-bin");
    install_failing_spago_launcher(&global_directory);
    install_failing_spago_launcher(&workspace.path().join("src/node_modules/.bin"));
    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let search_directories =
        std::iter::once(global_directory).chain(env::split_paths(&inherited_path));
    let path = env::join_paths(search_directories).unwrap();

    for current_directory in ["", "src"] {
        fs::remove_file(workspace.path().join("spago-calls")).unwrap();
        let mut command = workspace.command_builder(current_directory, &["build", "--quiet"]);
        command.env("PATH", &path).env_remove("IRIS_SPAGO");
        let output = command.output().unwrap();
        assert_success(&output);
        assert!(workspace.path().join("output/Main/index.js").is_file());
        workspace.assert_spago_calls(current_directory, &[&["fetch", "-p", "example"]]);
        fs::remove_dir_all(workspace.path().join("output")).unwrap();
    }
}

#[test]
fn ignores_spago_installed_above_the_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "project/spago.yaml",
        "workspace: {}\npackage:\n  name: example\n  dependencies: []\n",
    );
    workspace.write("project/src/Main.purs", "module Main where\nvalue = 42\n");
    install_failing_spago_launcher(&workspace.path().join("node_modules/.bin"));
    let global_directory = workspace.path().join("global-bin");
    install_spago_launcher(&global_directory, Path::new(env!("CARGO_BIN_EXE_spago-e2e")));
    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let search_directories =
        std::iter::once(global_directory).chain(env::split_paths(&inherited_path));
    let path = env::join_paths(search_directories).unwrap();

    let mut command = workspace.command_builder("project/src", &["build", "--quiet"]);
    command.env("PATH", path).env_remove("IRIS_SPAGO");
    let output = command.output().unwrap();
    assert_success(&output);
    assert!(workspace.path().join("project/output/Main/index.js").is_file());
    workspace.assert_spago_calls("project/src", &[&["fetch", "-p", "example"]]);
}

#[cfg(windows)]
#[test]
fn discovers_windows_spago_launchers_and_preserves_explicit_overrides() {
    struct LauncherCase {
        name: &'static str,
        path_directories: &'static [&'static [&'static str]],
        use_explicit_override: bool,
        expect_command_script: bool,
    }

    let cases = [
        LauncherCase {
            name: "command script only",
            path_directories: &[&["spago.cmd"]],
            use_explicit_override: false,
            expect_command_script: true,
        },
        LauncherCase {
            name: "native executable only",
            path_directories: &[&["spago.exe"]],
            use_explicit_override: false,
            expect_command_script: false,
        },
        LauncherCase {
            name: "earlier PATH directory preferred",
            path_directories: &[&["spago.exe"], &["spago.cmd"]],
            use_explicit_override: false,
            expect_command_script: false,
        },
        LauncherCase {
            name: "earlier command script preferred over later native executable",
            path_directories: &[&["spago.cmd"], &["spago.exe"]],
            use_explicit_override: false,
            expect_command_script: true,
        },
        LauncherCase {
            name: "native executable preferred in the same directory",
            path_directories: &[&["spago.exe", "spago.cmd"]],
            use_explicit_override: false,
            expect_command_script: false,
        },
        LauncherCase {
            name: "explicit override preferred",
            path_directories: &[&["spago.cmd"]],
            use_explicit_override: true,
            expect_command_script: false,
        },
    ];

    for case in cases {
        let workspace = TestWorkspace::empty();
        let explicit_executable = workspace.path().join("explicit tool with spaces.exe");
        fs::copy(env!("CARGO_BIN_EXE_spago-e2e"), &explicit_executable).unwrap();
        let mut path_directories = Vec::new();
        for (index, launchers) in case.path_directories.iter().enumerate() {
            let directory = workspace.path().join(format!("tools {index} with spaces"));
            fs::create_dir(&directory).unwrap();
            if launchers.contains(&"spago.exe") {
                fs::copy(&explicit_executable, directory.join("spago.exe")).unwrap();
            }
            if launchers.contains(&"spago.cmd") {
                install_spago_launcher(&directory, &explicit_executable);
            }
            path_directories.push(directory);
        }
        let mut command = workspace.command_builder("", &["new", "--name", "example"]);
        command.env("PATH", env::join_paths(&path_directories).unwrap()).env_remove("IRIS_SPAGO");
        if case.use_explicit_override {
            install_failing_spago_launcher(&workspace.path().join("node_modules/.bin"));
            command.env("IRIS_SPAGO", &explicit_executable);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            case.name,
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(workspace.read("spago.yaml").contains("registry: 81.1.0"), "{}", case.name);
        let calls = workspace.read("spago-calls");
        let (directory, arguments) = calls.trim_end().split_once('\t').expect(case.name);
        assert_eq!(
            fs::canonicalize(directory).unwrap(),
            fs::canonicalize(workspace.path()).unwrap(),
            "{}",
            case.name,
        );
        assert_eq!(arguments, "registry\tpackage-sets\t--latest\t--json\t--quiet", "{}", case.name,);
        for (index, directory) in path_directories.iter().enumerate() {
            let script_marker = directory.join("script-used");
            assert_eq!(
                script_marker.exists(),
                index == 0 && case.expect_command_script,
                "{}",
                case.name,
            );
        }
    }
}

#[test]
fn does_not_create_a_partial_project_when_package_set_discovery_fails() {
    let workspace = TestWorkspace::empty();
    workspace.set_env("IRIS_E2E_SPAGO_FAIL", "registry unavailable");

    let output = workspace.command(&["new", "--name", "example"]);
    assert!(!output.status.success());
    assert!(!workspace.path().join("spago.yaml").exists());
    assert!(!workspace.path().join("src/Main.purs").exists());
    workspace
        .assert_spago_calls("", &[&["registry", "package-sets", "--latest", "--json", "--quiet"]]);
}

#[test]
fn does_not_overwrite_existing_project_files() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Main.purs", "original");

    let output = workspace.command(&["new", "--name", "example"]);
    assert!(!output.status.success());
    assert_eq!(workspace.read("src/Main.purs"), "original");
    assert!(!workspace.path().join("spago.yaml").exists());
}

#[test]
fn does_not_create_a_partial_project_when_a_source_directory_is_a_file() {
    let workspace = TestWorkspace::empty();
    workspace.write("src", "original");

    let output = workspace.command(&["new", "--name", "example"]);
    assert!(!output.status.success());
    assert_eq!(workspace.read("src"), "original");
    assert!(!workspace.path().join("spago.yaml").exists());
}

#[test]
fn does_not_create_a_nested_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: root
"#,
    );

    let output = workspace.command_in("packages/application", &["new", "--name", "application"]);
    assert!(!output.status.success());
    assert!(!workspace.path().join("packages/application/spago.yaml").exists());
}
