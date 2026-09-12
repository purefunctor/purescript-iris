#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use itertools::Itertools;

pub struct TestWorkspace {
    temporary: tempfile::TempDir,
}

impl TestWorkspace {
    pub fn empty() -> TestWorkspace {
        TestWorkspace { temporary: tempfile::tempdir().unwrap() }
    }

    pub fn path(&self) -> &Path {
        self.temporary.path()
    }

    pub fn write(&self, path: &str, content: &str) {
        let path = self.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.path().join(path)).unwrap()
    }

    pub fn summary(&self) -> String {
        let mut files = vec![];
        collect_files(self.path(), &mut files);
        files.sort();

        let mut summary = String::new();
        for path in files {
            let relative = path.strip_prefix(self.path()).unwrap();
            let relative = relative.to_string_lossy().replace('\\', "/");
            let content = fs::read_to_string(&path).unwrap();
            summary.push_str(&format!("--- {relative}\n{content}"));
            if !content.ends_with('\n') {
                summary.push('\n');
            }
        }
        summary
    }

    pub fn command(&self, arguments: &[&str]) -> Output {
        self.command_in("", arguments)
    }

    pub fn command_in(&self, directory: &str, arguments: &[&str]) -> Output {
        self.command_builder(directory, arguments).output().unwrap()
    }

    pub fn spawn(&self, arguments: &[&str]) -> Child {
        self.spawn_in("", arguments)
    }

    pub fn spawn_in(&self, directory: &str, arguments: &[&str]) -> Child {
        self.command_builder(directory, arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    pub fn command_builder(&self, directory: &str, arguments: &[&str]) -> Command {
        let current_directory = self.path().join(directory);
        fs::create_dir_all(&current_directory).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_iris-e2e"));
        command
            .args(arguments)
            .current_dir(current_directory)
            .env("IRIS_SPAGO", env!("CARGO_BIN_EXE_spago-e2e"))
            .env("IRIS_E2E_SPAGO", spago_executable())
            .env("IRIS_E2E_SPAGO_LOG", self.path().join("spago-calls"))
            .env("COLUMNS", "120")
            .env("NO_COLOR", "1");
        command
    }

    pub fn assert_spago_calls(&self, directory: &str, expected: &[&[&str]]) {
        let path = self.path().join("spago-calls");
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => panic!("failed to read Spago call log: {error}"),
        };
        let expected_directory = fs::canonicalize(self.path().join(directory)).unwrap();
        let mut actual_arguments = vec![];
        for line in source.lines() {
            let mut fields = line.split('\t');
            let actual_directory = fields.next().unwrap();
            let actual_directory = fs::canonicalize(actual_directory).unwrap();
            assert_eq!(actual_directory, expected_directory);
            actual_arguments.push(fields.collect_vec());
        }
        assert_eq!(actual_arguments, expected);
    }
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn spago_executable() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let executable = if cfg!(windows) { "spago.cmd" } else { "spago" };
    let path = manifest.join("tools/node_modules/.bin").join(executable);
    if !path.is_file() {
        panic!("missing pinned Spago executable {}; run `just e2e-prepare`", path.display());
    }
    path
}
