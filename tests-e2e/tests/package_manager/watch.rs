use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use itertools::Itertools;

use super::support::{TestWorkspace, assert_success};

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
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));
    watch.stop();
    insta::with_settings!({omit_expression => true}, {
    insta::assert_snapshot!("watch_single_package", normalized_watch_log(&watch.stdout()));
    });

    assert!(workspace.path().join("output/Main/index.js").is_file());
    insta::assert_snapshot!(workspace.spago_calls(), @r#".: spago "fetch" "-p" "application""#);
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
    watch.wait_for("initial workspace compilation", |stdout, _| {
        stdout.contains("Loaded 2 inputs: Application, Library")
    });
    watch.stop();
    insta::with_settings!({omit_expression => true}, {
    insta::assert_snapshot!("watch_workspace", normalized_watch_log(&watch.stdout()));
    });

    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"
    Application
    Library
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @r#"src: spago "fetch""#);
}

#[test]
fn rebuilds_after_edits_and_recovers_from_diagnostics() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nvalue = 42\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));

    workspace.write("src/Main.purs", "module Main where\n\nvalue = missing\n");
    watch.wait_for("diagnostic rebuild", |stdout, stderr| {
        stdout.contains("Rebuild completed with diagnostics") && stderr.contains("[NotInScope]")
    });

    workspace.write("src/Main.purs", "module Main where\n\nvalue = 43\n");
    watch.wait_for("recovered rebuild", |stdout, _| stdout.contains("Rebuild succeeded"));
    assert!(generated_module_contains(&workspace.path().join("output/Main/index.js"), "43"));
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
    watch.wait_for("recovery with diagnostics suppressed", |stdout, _| {
        stdout.contains("Rebuild succeeded")
    });
    assert!(workspace.path().join("output/Main/index.js").is_file());
}

#[test]
fn adds_and_removes_sources_without_deleting_outputs() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));
    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"Main");

    workspace.write("src/Extra.purs", "module Extra where\n\nvalue = 42\n");
    let extra_output = workspace.path().join("output/Extra/index.js");
    watch.wait_for("source addition", |stdout, _| stdout.contains("Changed 1 input: Extra"));
    assert!(extra_output.is_file());
    let metadata = workspace.normalized_metadata();
    insta::assert_snapshot!(metadata, @r#"
    --- output/Extra/metadata.json
    {
      "version": 1,
      "iris": "[VERSION]",
      "package": "application",
      "editable": true,
      "source": "[WORKSPACE]/src/Extra.purs",
      "foreign": null
    }
    --- output/Main/metadata.json
    {
      "version": 1,
      "iris": "[VERSION]",
      "package": "application",
      "editable": true,
      "source": "[WORKSPACE]/src/Main.purs",
      "foreign": null
    }
    "#);

    std::fs::remove_file(workspace.path().join("src/Extra.purs")).unwrap();
    watch.wait_for("source removal", |stdout, _| stdout.contains("Changed 1 input: Extra"));
    assert!(extra_output.is_file());
    assert_eq!(workspace.normalized_metadata(), metadata);
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
    });

    std::fs::remove_file(workspace.path().join("src/Extra.purs")).unwrap();
    watch.wait_for("diagnostic rebuild after source removal", |stdout, _| {
        stdout.contains("Rebuild completed with diagnostics")
    });
    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"
    Extra
    Main
    ");
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
    watch.wait_for("initial FFI compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));
    insta::assert_snapshot!(workspace.read("output/Main/foreign.js"), @"export const value = 42;");
    insta::assert_snapshot!(workspace.module_metadata("Main").unwrap()["foreign"], @r#""foreign.js""#);

    workspace.write("src/Main.js", "export const value = 43;\n");
    watch.wait_for("JavaScript FFI rebuild", |stdout, _| stdout.contains("Changed 1 input: Main"));
    insta::assert_snapshot!(workspace.read("output/Main/foreign.js"), @"export const value = 43;");

    std::fs::remove_file(workspace.path().join("src/Main.js")).unwrap();
    workspace.write("src/Main.jsx", "export const value = 44;\n");
    watch.wait_for("JSX FFI rebuild", |stdout, _| stdout.contains("Changed 1 input: Main"));
    insta::assert_snapshot!(workspace.read("output/Main/foreign.jsx"), @"export const value = 44;");
    insta::assert_snapshot!(workspace.module_metadata("Main").unwrap()["foreign"], @r#""foreign.jsx""#);
    insta::assert_snapshot!(workspace.read("output/Main/foreign.js"), @"export const value = 43;");
}

#[test]
fn waits_for_a_deleted_input_directory_and_builds_when_it_returns() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nvalue = 42\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    let output = workspace.path().join("output/Main/index.js");
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));
    let metadata = workspace.normalized_metadata();
    insta::assert_snapshot!(metadata, @r#"
    --- output/Main/metadata.json
    {
      "version": 1,
      "iris": "[VERSION]",
      "package": "application",
      "editable": true,
      "source": "[WORKSPACE]/src/Main.purs",
      "foreign": null
    }
    "#);

    std::fs::remove_dir_all(workspace.path().join("src")).unwrap();
    watch.wait_for("empty input state", |stdout, _| {
        stdout.contains("No input files; waiting for changes")
    });
    assert_eq!(workspace.normalized_metadata(), metadata);
    assert!(generated_module_contains(&output, "42"));

    workspace.write("src/Main.purs", "module Main where\n\nvalue = 43\n");
    watch.wait_for("recreated input compilation", |stdout, _| stdout.contains("Rebuild succeeded"));
    assert!(generated_module_contains(&output, "43"));
    assert_eq!(workspace.normalized_metadata(), metadata);
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
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));

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
    insta::assert_snapshot!(workspace.metadata_modules().join("\n"), @"Main");
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
    });
    insta::assert_snapshot!(run_query(&workspace, "wait"), @"
    $ iris watch query wait
    [0] Build succeeded.
    ");

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
    insta::assert_snapshot!(run_query(&workspace, "wait"), @"
    $ iris watch query wait
    [0] Build succeeded.
    ");
    watch.wait_for("module added to a dependency", |stdout, _| {
        stdout.contains("Rebuild succeeded") && stdout.contains("Dependency")
    });
    assert!(!watch.stdout().contains("Unselected"), "{}", watch.stdout());
    assert!(!workspace.path().join("output/Unselected").exists());
    insta::assert_snapshot!(workspace.normalized_metadata(), @r#"
    --- output/Dependency/metadata.json
    {
      "version": 1,
      "iris": "[VERSION]",
      "package": "library",
      "editable": true,
      "source": "[WORKSPACE]/packages/library/src/Dependency.purs",
      "foreign": null
    }
    --- output/Library/metadata.json
    {
      "version": 1,
      "iris": "[VERSION]",
      "package": "library",
      "editable": true,
      "source": "[WORKSPACE]/packages/library/src/Library.purs",
      "foreign": null
    }
    --- output/Main/metadata.json
    {
      "version": 1,
      "iris": "[VERSION]",
      "package": "application",
      "editable": true,
      "source": "[WORKSPACE]/src/Main.purs",
      "foreign": null
    }
    "#);
    insta::assert_snapshot!(workspace.spago_calls(), @r#".: spago "fetch" "-p" "application""#);
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
    watch.wait_for("custom output compilation", |stdout, _| stdout.contains("Build succeeded"));
    assert!(workspace.path().join("src/generated/Main/index.js").is_file());
}

#[cfg(target_os = "linux")]
#[test]
fn watches_sources_without_watching_the_output_directory() {
    use std::collections::BTreeSet;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    fn directory_inodes(directory: &Path, inodes: &mut BTreeSet<u64>) {
        inodes.insert(fs::metadata(directory).unwrap().ino());
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                directory_inodes(&path, inodes);
            }
        }
    }

    fn watched_inodes(pid: u32) -> BTreeSet<u64> {
        let mut inodes = BTreeSet::new();
        for entry in fs::read_dir(format!("/proc/{pid}/fd")).unwrap() {
            let entry = entry.unwrap();
            let target = fs::read_link(entry.path());
            if !target.is_ok_and(|target| target == Path::new("anon_inode:inotify")) {
                continue;
            }
            let descriptor = entry.file_name();
            let information = Path::new(&format!("/proc/{pid}/fdinfo")).join(descriptor);
            let information = fs::read_to_string(information).unwrap();
            for watch in information.lines().filter(|line| line.starts_with("inotify ")) {
                let inode = watch.split_whitespace().find_map(|field| field.strip_prefix("ino:"));
                inodes.insert(u64::from_str_radix(inode.unwrap(), 16).unwrap());
            }
        }
        inodes
    }

    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n");
    workspace.write("output/Stale/index.js", "export {};\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Loaded 1 input: Main"));

    let watched = watched_inodes(watch.child.id());
    let sources = fs::metadata(workspace.path().join("src")).unwrap().ino();
    assert!(watched.contains(&sources), "{watched:?}");
    let mut output = BTreeSet::new();
    directory_inodes(&workspace.path().join("output"), &mut output);
    assert!(output.is_disjoint(&watched), "{:?}", output.intersection(&watched).collect_vec());
}

#[test]
fn reports_output_failures_separately_and_recovers_on_a_later_change() {
    let workspace = TestWorkspace::empty();
    workspace
        .write("spago.yaml", "workspace: {}\npackage:\n  name: application\n  dependencies: []\n");
    workspace.write("src/Main.purs", "module Main where\n\nforeign import value :: Int\n");
    workspace.write("src/Main.js", "export const value = 42;\n");

    let mut watch = WatchProcess::new(workspace.spawn(&["watch"]));
    watch.wait_for("initial compilation", |stdout, _| stdout.contains("Build succeeded"));

    let foreign_output = workspace.path().join("output/Main/foreign.js");
    std::fs::remove_file(&foreign_output).unwrap();
    std::fs::create_dir(&foreign_output).unwrap();
    workspace.write(
        "src/Main.js",
        r#"export const value = 43;
"#,
    );
    watch.wait_for("output failure", |_, stderr| stderr.contains("Watch build failed:"));
    let settled = run_query(&workspace, "wait");
    insta::assert_snapshot!(workspace.normalize_output(settled.as_bytes()), @"
    $ iris watch query wait
    [0] Build failed: failed to write [WORKSPACE]/output/Main/foreign.js: Is a directory (os error 21)
    ");
    assert!(!watch.stdout().contains("completed with diagnostics"));
    assert!(workspace.module_metadata("Main").is_some());

    std::fs::remove_dir(&foreign_output).unwrap();
    workspace.write(
        "src/Main.js",
        r#"export const value = 44;
"#,
    );
    watch.wait_for("recovery", |stdout, _| stdout.contains("Rebuild succeeded"));
    assert!(generated_module_contains(&foreign_output, "44"));
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
    output: Arc<WatchOutput>,
    readers: Vec<JoinHandle<()>>,
    stdout_cursor: usize,
    stderr_cursor: usize,
}

#[derive(Default)]
struct WatchOutput {
    streams: Mutex<WatchStreams>,
    changed: Condvar,
}

impl WatchOutput {
    fn lock(&self) -> MutexGuard<'_, WatchStreams> {
        self.streams.lock().expect("invariant violated: watch output capture is poisoned")
    }
}

#[derive(Default)]
struct WatchStreams {
    stdout: String,
    stderr: String,
    closed: usize,
}

impl WatchProcess {
    fn new(mut child: Child) -> WatchProcess {
        let output = Arc::new(WatchOutput::default());
        let stdout =
            child.stdout.take().expect("invariant violated: watch process stdout is not piped");
        let stderr =
            child.stderr.take().expect("invariant violated: watch process stderr is not piped");
        let readers = vec![
            spawn_reader(stdout, Arc::clone(&output), |streams| &mut streams.stdout),
            spawn_reader(stderr, Arc::clone(&output), |streams| &mut streams.stderr),
        ];
        WatchProcess { child, output, readers, stdout_cursor: 0, stderr_cursor: 0 }
    }

    fn wait_for(&mut self, description: &str, predicate: impl Fn(&str, &str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let output = Arc::clone(&self.output);
        let mut streams = output.lock();
        loop {
            let new_stdout = &streams.stdout[self.stdout_cursor..];
            let new_stderr = &streams.stderr[self.stderr_cursor..];
            if predicate(new_stdout, new_stderr) {
                self.stdout_cursor = streams.stdout.len();
                self.stderr_cursor = streams.stderr.len();
                return;
            }
            let failure = if streams.closed == self.readers.len() {
                "exited before"
            } else if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                streams = output.changed.wait_timeout(streams, remaining).unwrap().0;
                continue;
            } else {
                "did not reach"
            };
            drop(streams);
            self.stop();
            panic!(
                "watch {failure} {description} with status {:?}\nstdout:\n{}\nstderr:\n{}",
                self.child.try_wait().unwrap(),
                self.stdout(),
                self.stderr()
            );
        }
    }

    fn stdout(&self) -> String {
        String::clone(&self.output.lock().stdout)
    }

    fn stderr(&self) -> String {
        String::clone(&self.output.lock().stderr)
    }

    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
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
    output: Arc<WatchOutput>,
    capture: fn(&mut WatchStreams) -> &mut String,
) -> JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let line = line.unwrap();
            let mut streams = output.lock();
            let captured = capture(&mut streams);
            captured.push_str(&line);
            captured.push('\n');
            output.changed.notify_all();
        }
        output.lock().closed += 1;
        output.changed.notify_all();
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
    assert_success(&signature);
    let response: serde_json::Value = serde_json::from_slice(&signature.stdout).unwrap();
    let response = serde_json::to_string_pretty(&response).unwrap();
    insta::assert_snapshot!(response, @r#"
    {
      "kind": "result",
      "value": {
        "declarations": [
          {
            "documentation": "The size of a shape.",
            "signature": "size :: Shape -> Int"
          }
        ]
      }
    }
    "#);
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
    let second = workspace.report(&workspace.command(&["watch"]));
    insta::with_settings!({filters => vec![(r"\(process \d+\)", "(process [PID])")]}, {
        insta::assert_snapshot!(second, @"
        status: 1
        --- stdout
        --- stderr
        another `iris watch` (process [PID]) is already writing to [WORKSPACE]/output
        ");
    });

    first.stop();
    let orphaned = run_query(&workspace, "wait");
    insta::assert_snapshot!(workspace.normalize_output(orphaned.as_bytes()), @"
    $ iris watch query wait
    [4] no `iris watch` is running for [WORKSPACE]/output; start one with `iris watch`, or retry if it is starting
    ");

    let mut restarted = WatchProcess::new(workspace.spawn(&["watch"]));
    restarted.wait_for("compilation after restart", |stdout, _| stdout.contains("Build succeeded"));
    insta::assert_snapshot!(run_query(&workspace, "wait"), @"
    $ iris watch query wait
    [0] Build succeeded.
    ");
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
    insta::assert_snapshot!(watch.child.wait().unwrap().code().unwrap(), @"143");
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
    let transcript = workspace.transcript(&[
        &["watch", "--output", "generated", "query", "wait"],
        &["watch", "query", "--output", "generated", "wait"],
        &["watch", "query", "wait"],
    ]);
    insta::assert_snapshot!(transcript, @"
    $ iris watch --output generated query wait
    status: 0
    --- stdout
    Build succeeded.
    --- stderr
    $ iris watch query --output generated wait
    status: 0
    --- stdout
    Build succeeded.
    --- stderr
    $ iris watch query wait
    status: 4
    --- stdout
    --- stderr
    no `iris watch` is running for [WORKSPACE]/output; start one with `iris watch`, or retry if it is starting
    ");

    workspace.write("spago.yaml", "workspace: [\n");
    let absolute_output = workspace.path().join("generated");
    for output in ["../generated", absolute_output.to_str().unwrap()] {
        let answer = workspace.command_in("src", &["watch", "query", "--output", output, "wait"]);
        insta::allow_duplicates! {
            insta::assert_snapshot!(workspace.report(&answer), @"
            status: 0
            --- stdout
            Build succeeded.
            --- stderr
            ");
        }
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
