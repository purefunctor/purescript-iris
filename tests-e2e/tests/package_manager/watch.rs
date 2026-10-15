use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use itertools::Itertools;

use super::support::TestWorkspace;

#[test]
fn watches_a_single_package_with_real_spago() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

value = 42
"#,
    );

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    let output = workspace.path().join("output/Main/index.js");
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 1 input: Main") && output.is_file()
    });
    watch.stop();
    insta::with_settings!({omit_expression => true}, {
    insta::assert_snapshot!("watch_single_package", normalized_watch_log(&watch.stdout()));
    });

    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}

#[test]
fn watches_the_whole_workspace_from_a_root_package_subdirectory() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write("src/Application.purs", "module Application where\n");
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write("packages/library/src/Library.purs", "module Library where\n");

    let mut watch = WatchProcess::new(workspace.spawn_in("src", &["watch"]));
    let outputs = [
        workspace.path().join("output/Application/index.js"),
        workspace.path().join("output/Library/index.js"),
    ];
    watch.wait_for("initial workspace compilation", |stdout, _| {
        stdout.contains("Loaded 2 inputs: Application, Library")
            && outputs.iter().all(|output| output.is_file())
    });
    watch.stop();
    insta::with_settings!({omit_expression => true}, {
    insta::assert_snapshot!("watch_workspace", normalized_watch_log(&watch.stdout()));
    });

    workspace.assert_spago_calls("src", &[&["fetch"]]);
}

#[test]
fn rebuilds_after_edits_and_recovers_from_diagnostics() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nvalue = 42\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    let output = workspace.path().join("output/Main/index.js");
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 1 input: Main") && output.is_file()
    });

    workspace.write("src/Main.purs", "module Main where\n\nvalue = missing\n");
    watch.wait_for("diagnostic rebuild", |stdout, stderr| {
        stdout.contains("Rebuild completed with diagnostics") && stderr.contains("[NotInScope]")
    });

    workspace.write("src/Main.purs", "module Main where\n\nvalue = 43\n");
    watch.wait_for("recovered rebuild", |stdout, _| {
        stdout.contains("Rebuild succeeded") && generated_module_contains(&output, "43")
    });
    watch.stop();
    let mut settings = insta::Settings::clone_current();
    settings.set_strip_ansi_escape_codes(true);
    settings.add_filter(r"(?s)\A.*?(Error! ·)", "$1");
    let _settings = settings.bind_to_scope();
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("watch_diagnostic_recovery", watch.stderr());
    });
}

#[test]
fn suppresses_diagnostics_without_hiding_watch_summaries() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nvalue = missing\n");

    let child = workspace.spawn(&["watch", "--no-diagnostics"]);
    let mut watch = WatchProcess::new(child);
    watch.wait_for("suppressed diagnostic build", |stdout, _| {
        stdout.contains("Build completed with diagnostics")
    });
    assert!(!watch.stderr().contains("[NotInScope]"));

    workspace.write("src/Main.purs", "module Main where\n\nvalue = 42\n");
    let output = workspace.path().join("output/Main/index.js");
    watch.wait_for("recovery with diagnostics suppressed", |stdout, _| {
        stdout.contains("Rebuild succeeded") && output.is_file()
    });
}

#[test]
fn adds_and_removes_sources_without_deleting_outputs() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 1 input: Main") && workspace.metadata_modules() == ["Main"]
    });

    workspace.write("src/Extra.purs", "module Extra where\n\nvalue = 42\n");
    let extra_output = workspace.path().join("output/Extra/index.js");
    watch.wait_for("source addition", |stdout, _| {
        stdout.contains("Changed 1 input: Extra")
            && workspace.module_metadata("Extra").is_some()
            && extra_output.is_file()
    });
    let extra = workspace.module_metadata("Extra").unwrap();
    assert_eq!(extra["package"], "application");

    std::fs::remove_file(workspace.path().join("src/Extra.purs")).unwrap();
    watch.wait_for("source removal", |stdout, _| stdout.contains("Changed 1 input: Extra"));
    assert!(extra_output.is_file());
    assert_eq!(workspace.module_metadata("Extra"), Some(extra));
}

#[test]
fn keeps_the_previous_build_when_a_deleted_module_is_still_imported() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Extra (value)

main = value
"#,
    );
    workspace.write(
        "src/Extra.purs",
        r#"module Extra where

value = 42
"#,
    );

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 2 inputs: Extra, Main")
            && workspace.metadata_modules() == ["Extra", "Main"]
    });

    std::fs::remove_file(workspace.path().join("src/Extra.purs")).unwrap();
    watch.wait_for("diagnostic rebuild after source removal", |stdout, _| {
        stdout.contains("Rebuild completed with diagnostics")
    });
    assert_eq!(workspace.metadata_modules(), ["Extra", "Main"]);
    assert!(workspace.path().join("output/Extra/index.js").is_file());
    assert!(workspace.path().join("output/Main/index.js").is_file());
}

#[test]
fn rebuilds_for_ffi_changes_and_records_the_foreign_module() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nforeign import value :: Int\n");
    workspace.write("src/Main.js", "export const value = 42;\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    let javascript_output = workspace.path().join("output/Main/foreign.js");
    watch.wait_for("initial FFI compilation", |stdout, _| {
        stdout.contains("Loaded 1 input: Main")
            && generated_module_contains(&javascript_output, "42")
            && foreign_module(&workspace).as_deref() == Some("foreign.js")
    });

    workspace.write("src/Main.js", "export const value = 43;\n");
    watch.wait_for("JavaScript FFI rebuild", |stdout, _| {
        stdout.contains("Changed 1 input: Main")
            && generated_module_contains(&javascript_output, "43")
    });

    std::fs::remove_file(workspace.path().join("src/Main.js")).unwrap();
    workspace.write("src/Main.jsx", "export const value = 44;\n");
    let jsx_output = workspace.path().join("output/Main/foreign.jsx");
    watch.wait_for("JSX FFI rebuild", |stdout, _| {
        stdout.contains("Changed 1 input: Main")
            && generated_module_contains(&jsx_output, "44")
            && foreign_module(&workspace).as_deref() == Some("foreign.jsx")
    });
    assert!(generated_module_contains(&javascript_output, "43"));
}

#[test]
fn waits_for_a_deleted_input_directory_and_builds_when_it_returns() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nvalue = 42\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    let output = workspace.path().join("output/Main/index.js");
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 1 input: Main") && workspace.module_metadata("Main").is_some()
    });
    let metadata = workspace.module_metadata("Main");

    std::fs::remove_dir_all(workspace.path().join("src")).unwrap();
    watch.wait_for("empty input state", |stdout, _| {
        stdout.contains("No input files; waiting for changes")
    });
    assert_eq!(workspace.module_metadata("Main"), metadata);
    assert!(generated_module_contains(&output, "42"));

    workspace.write("src/Main.purs", "module Main where\n\nvalue = 43\n");
    watch.wait_for("recreated input compilation", |stdout, _| {
        stdout.contains("Rebuild succeeded") && generated_module_contains(&output, "43")
    });
    assert_eq!(workspace.module_metadata("Main"), metadata);
}

#[test]
fn reports_a_source_that_two_packages_claim_and_recovers_when_it_is_removed() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where
"#,
    );
    workspace.write(
        "src/vendored/spago.yaml",
        r#"package:
  name: vendored
  dependencies: []
"#,
    );

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 1 input: Main") && workspace.metadata_modules() == ["Main"]
    });

    let claimed = workspace.path().join("src/vendored/src/Vendored.purs");
    workspace.write(
        "src/vendored/src/Vendored.purs",
        r#"module Vendored where
"#,
    );
    watch.wait_for("ownership failure", |_, stderr| {
        stderr.contains("Watch build failed: source file ")
            && stderr.contains("is claimed by packages 'application' and 'vendored'")
    });
    assert!(!workspace.path().join("output/Vendored").exists());

    std::fs::remove_file(claimed).unwrap();
    watch.wait_for("recovered rebuild", |stdout, _| stdout.contains("Rebuild succeeded"));
    assert_eq!(workspace.metadata_modules(), ["Main"]);
}

#[test]
fn ignores_unselected_packages_and_records_new_modules_of_dependencies() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: [library]
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where
"#,
    );
    workspace.write(
        "packages/library/spago.yaml",
        r#"package:
  name: library
  dependencies: []
"#,
    );
    workspace.write(
        "packages/library/src/Library.purs",
        r#"module Library where
"#,
    );
    workspace.write(
        "packages/tool/spago.yaml",
        r#"package:
  name: tool
  dependencies: []
"#,
    );
    workspace.write(
        "packages/tool/src/Tool.purs",
        r#"module Tool where
"#,
    );

    let mut watch = WatchProcess::new(workspace.spawn(&["watch", "--package", "application"]));
    watch.wait_for("initial compilation", |stdout, _| {
        stdout.contains("Loaded 2 inputs: Library, Main")
            && workspace.metadata_modules() == ["Library", "Main"]
    });
    let settled = workspace.command(&["watch", "query", "wait"]);
    assert_eq!(String::from_utf8_lossy(&settled.stdout), "Build succeeded.\n");

    workspace.write(
        "packages/tool/src/Unselected.purs",
        r#"module Unselected where
"#,
    );
    workspace.write(
        "packages/library/src/Dependency.purs",
        r#"module Dependency where
"#,
    );
    let rescanned = workspace.command(&["watch", "query", "wait"]);
    assert_eq!(String::from_utf8_lossy(&rescanned.stdout), "Build succeeded.\n");
    watch.wait_for("module added to a dependency", |stdout, _| {
        stdout.contains("Rebuild succeeded") && stdout.contains("Dependency")
    });
    assert!(!watch.stdout().contains("Unselected"), "{}", watch.stdout());
    assert!(!workspace.path().join("output/Unselected").exists());
    assert_eq!(workspace.metadata_modules(), ["Dependency", "Library", "Main"]);
    let dependency = workspace.module_metadata("Dependency").unwrap();
    assert_eq!(dependency["package"], "library");
    assert_eq!(dependency["editable"], true);
    workspace.assert_spago_calls("", &[&["fetch", "-p", "application"]]);
}

#[test]
fn resolves_relative_output_from_the_invocation_directory_and_excludes_it() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n");
    workspace.write("src/generated/Ignored.purs", "this is not PureScript\n");

    let child =
        workspace.spawn_in("src", &["watch", "--package", "application", "--output", "generated"]);
    let mut watch = WatchProcess::new(child);
    let output = workspace.path().join("src/generated/Main/index.js");
    watch.wait_for("custom output compilation", |stdout, _| {
        stdout.contains("Build succeeded") && output.is_file()
    });
}

#[test]
fn reports_output_failures_separately_and_recovers_on_a_later_change() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nforeign import value :: Int\n");
    workspace.write("src/Main.js", "export const value = 42;\n");

    let child = workspace.spawn(&["watch", "--quiet"]);
    let mut watch = WatchProcess::new(child);
    watch.wait_for("initial compilation", |_, _| workspace.module_metadata("Main").is_some());

    let foreign_output = workspace.path().join("output/Main/foreign.js");
    std::fs::remove_file(&foreign_output).unwrap();
    std::fs::create_dir(&foreign_output).unwrap();
    workspace.write(
        "src/Main.js",
        r#"export const value = 43;
"#,
    );
    watch.wait_for("output failure", |_, stderr| stderr.contains("Watch build failed:"));
    assert!(!watch.stderr().contains("completed with diagnostics"));
    assert!(workspace.module_metadata("Main").is_some());

    thread::sleep(Duration::from_millis(500));
    let settled_failures = watch.stderr().matches("Watch build failed:").count();
    thread::sleep(Duration::from_millis(500));
    assert_eq!(watch.stderr().matches("Watch build failed:").count(), settled_failures);

    std::fs::remove_dir(&foreign_output).unwrap();
    workspace.write(
        "src/Main.js",
        r#"export const value = 44;
"#,
    );
    watch.wait_for("recovery", |_, _| generated_module_contains(&foreign_output, "44"));
}

fn foreign_module(workspace: &TestWorkspace) -> Option<String> {
    let metadata = workspace.module_metadata("Main")?;
    metadata["foreign"].as_str().map(str::to_owned)
}

fn generated_module_contains(path: &Path, expected: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|source| source.contains(expected))
}

fn normalized_watch_log(stdout: &str) -> String {
    let mut lines = stdout.lines().map(|line| {
        let prefix = line.as_bytes().get(..8);
        let is_plain_timestamp = prefix.is_some_and(|timestamp| {
            timestamp[2] == b':'
                && timestamp[5] == b':'
                && timestamp
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
        });
        if is_plain_timestamp && line.as_bytes().get(8..10) == Some(b"  ") {
            let message = &line[10..];
            let message = message.rsplit_once(" in ").map_or_else(
                || message.to_owned(),
                |(message, _)| format!("{message} in [DURATION]"),
            );
            return format!("[TIME] {message}");
        }
        let Some((timestamp, message)) = line.split_once("] ") else {
            return line.to_owned();
        };
        let timestamp = timestamp.as_bytes();
        let is_timestamp = timestamp.len() == 9
            && timestamp[0] == b'['
            && timestamp[3] == b':'
            && timestamp[6] == b':'
            && timestamp[1..3].iter().all(u8::is_ascii_digit)
            && timestamp[4..6].iter().all(u8::is_ascii_digit)
            && timestamp[7..9].iter().all(u8::is_ascii_digit);
        if is_timestamp { format!("[TIME] {message}") } else { line.to_owned() }
    });
    lines.join("\n")
}

struct WatchProcess {
    child: Child,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    stdout_cursor: usize,
    stderr_cursor: usize,
}

impl WatchProcess {
    fn new(mut child: Child) -> WatchProcess {
        let stdout = Arc::new(Mutex::new(String::new()));
        let stderr = Arc::new(Mutex::new(String::new()));
        let stdout_reader = spawn_reader(
            child.stdout.take().expect("invariant violated: watch process stdout is not piped"),
            Arc::clone(&stdout),
        );
        let stderr_reader = spawn_reader(
            child.stderr.take().expect("invariant violated: watch process stderr is not piped"),
            Arc::clone(&stderr),
        );
        WatchProcess {
            child,
            stdout,
            stderr,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
            stdout_cursor: 0,
            stderr_cursor: 0,
        }
    }

    fn wait_for(&mut self, description: &str, predicate: impl Fn(&str, &str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let stdout = self.stdout();
            let stderr = self.stderr();
            let new_stdout = &stdout[self.stdout_cursor..];
            let new_stderr = &stderr[self.stderr_cursor..];
            if predicate(new_stdout, new_stderr) {
                self.stdout_cursor = stdout.len();
                self.stderr_cursor = stderr.len();
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!(
                    "watch exited before {description} with status {status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
                );
            }
            if Instant::now() >= deadline {
                self.stop();
                panic!(
                    "watch did not reach {description} within 30 seconds\nstdout:\n{stdout}\nstderr:\n{stderr}"
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn stdout(&self) -> String {
        let stdout =
            self.stdout.lock().expect("invariant violated: watch stdout capture is poisoned");
        String::clone(&stdout)
    }

    fn stderr(&self) -> String {
        let stderr =
            self.stderr.lock().expect("invariant violated: watch stderr capture is poisoned");
        String::clone(&stderr)
    }

    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.stdout_reader.take() {
            reader.join().unwrap();
        }
        if let Some(reader) = self.stderr_reader.take() {
            reader.join().unwrap();
        }
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_reader(
    stream: impl std::io::Read + Send + 'static,
    capture: Arc<Mutex<String>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let line = line.unwrap();
            let mut capture =
                capture.lock().expect("invariant violated: watch output capture is poisoned");
            capture.push_str(&line);
            capture.push('\n');
        }
    })
}

#[test]
fn answers_queries_from_the_running_watcher() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Shapes.purs",
        r#"module Shapes where

-- | A shape with a size.
data Shape = Square Int | Circle Int

-- | The size of a shape.
size :: Shape -> Int
size (Square side) = side
size (Circle radius) = radius

newtype Size = Size Int

class Area a where
  area :: a -> Int

instance Area Shape where
  area shape = size shape
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Shapes (Shape(..), size)

main :: Int
main = size (Square 2)

circle :: Int
circle = size (Circle 1)

square :: Int
square = size (Square 3)
"#,
    );
    workspace.write(
        "src/App.purs",
        r#"module App where

import Main (main)

app :: Int
app = main
"#,
    );
    workspace.write(
        "src/Empty.purs",
        r#"module Empty where
"#,
    );
    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));

    let mut transcript = String::new();
    for query in [
        "signature Shapes.size",
        "signature Shapes.Square",
        "module Shapes",
        "module Empty",
        "definition Shapes.size",
        "references Shapes.size",
        "javascript Main",
        "diagnostics",
        "signature Shapes.missing",
        "signature Shapes.Size",
        "signature value Shapes.Size",
        "definition type Shapes.Size",
        "signature type Shapes.size",
        "instances class Shapes.Area",
        "instances type Shapes.Shape",
        "instances class Shapes.Shape",
        "dependents Shapes",
        "search siz",
    ] {
        transcript.push_str(&run_query(&workspace, query));
    }
    let signature = workspace.command(&["watch", "query", "--json", "signature", "Shapes.size"]);
    assert_eq!(signature.status.code(), Some(0), "{signature:?}");
    let response: serde_json::Value = serde_json::from_slice(&signature.stdout).unwrap();
    assert_eq!(response["kind"], "result");
    assert_eq!(
        response["value"],
        serde_json::json!({
            "declarations": [{
                "signature": "size :: Shape -> Int",
                "documentation": "The size of a shape.",
            }],
        })
    );
    // Built-in modules are materialized files, as in the language server, so their
    // declarations have locations an agent can read.
    let definition = workspace.command(&["watch", "query", "definition", "Prim.Int"]);
    let location = String::from_utf8_lossy(&definition.stdout);
    let path = location.trim_end().rsplitn(3, ':').nth(2).unwrap();
    assert!(std::fs::read_to_string(path).unwrap().contains("Int"), "{location}");

    // A query sent right after an edit must not answer from the build before it.
    workspace.write(
        "src/Main.purs",
        r#"module Main where

import Shapes (size, size)

main :: Int
main = "oops"

foreign import missing :: Int
"#,
    );
    // An operator whose target does not resolve is left unchecked, and is still listed.
    workspace.write(
        "src/Broken.purs",
        r#"module Broken where

infixl 6 missing as <+>
"#,
    );
    for query in ["wait", "diagnostics Main", "javascript Main", "module Broken"] {
        transcript.push_str(&run_query(&workspace, query));
    }
    insta::assert_snapshot!("watch_queries", transcript);
}

#[test]
fn refuses_a_second_watcher_and_recovers_after_a_crash() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where
"#,
    );

    let mut first = WatchProcess::new(workspace.spawn(&["watch"]));
    first.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));
    let second = workspace.command(&["watch"]);
    assert_eq!(second.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&second.stderr).contains("is already writing to"));

    first.stop();
    let orphaned = workspace.command(&["watch", "query", "wait"]);
    assert_eq!(orphaned.status.code(), Some(4), "{orphaned:?}");

    let mut restarted = WatchProcess::new(workspace.spawn(&["watch"]));
    restarted.wait_for("compilation after restart", |stdout, _| stdout.contains("Build succeeded"));
    let expected = r#"$ iris watch query wait
[0] Build succeeded.

"#;
    assert_eq!(run_query(&workspace, "wait"), expected);
}

#[cfg(unix)]
#[test]
fn removes_the_socket_file_when_terminated() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where
"#,
    );

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));
    let socket_file = workspace.path().join("output/.iris-watch");
    assert!(socket_file.is_file());

    let terminate = std::process::Command::new("kill")
        .args(["-TERM", &watch.child.id().to_string()])
        .status()
        .unwrap();
    assert!(terminate.success());
    assert_eq!(watch.child.wait().unwrap().code(), Some(143));
    assert!(!socket_file.exists());
}

#[test]
fn queries_a_watcher_with_a_custom_output_directory() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        r#"workspace: {}
package:
  name: application
  dependencies: []
"#,
    );
    workspace.write(
        "src/Main.purs",
        r#"module Main where
"#,
    );

    let mut watch = WatchProcess::new(workspace.spawn(&["watch", "--output", "generated"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));
    let from_watch_flags = workspace.command(&["watch", "--output", "generated", "query", "wait"]);
    assert_eq!(String::from_utf8_lossy(&from_watch_flags.stdout), "Build succeeded.\n");
    let from_query_flags = workspace.command(&["watch", "query", "--output", "generated", "wait"]);
    assert_eq!(String::from_utf8_lossy(&from_query_flags.stdout), "Build succeeded.\n");
    let default_output = workspace.command(&["watch", "query", "wait"]);
    assert_eq!(default_output.status.code(), Some(4));

    workspace.write("spago.yaml", "workspace: [\n");
    let absolute_output = workspace.path().join("generated");
    for output in ["../generated", absolute_output.to_str().unwrap()] {
        let answer = workspace.command_in("src", &["watch", "query", "--output", output, "wait"]);
        assert_eq!(answer.status.code(), Some(0), "{answer:?}");
        assert_eq!(String::from_utf8_lossy(&answer.stdout), "Build succeeded.\n");
    }
}

/// Runs `iris watch query` and records the command, its exit status, and its output.
fn run_query(workspace: &TestWorkspace, query: &str) -> String {
    let mut arguments = vec!["watch", "query"];
    arguments.extend(query.split(' '));
    let output = workspace.command(&arguments);
    let status = output.status.code().map_or_else(|| "signal".to_string(), |code| code.to_string());
    format!(
        "$ iris watch query {query}\n[{status}] {}{}\n",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
