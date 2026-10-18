#![allow(dead_code)]

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::LazyLock;

use itertools::Itertools;
use regex::Regex;
use url::Url;

pub struct TestWorkspace {
    temporary: tempfile::TempDir,
    process_temporary: tempfile::TempDir,
    environment: RefCell<Vec<(String, String)>>,
}

impl TestWorkspace {
    pub fn empty() -> TestWorkspace {
        TestWorkspace {
            temporary: tempfile::tempdir().unwrap(),
            process_temporary: tempfile::tempdir().unwrap(),
            environment: RefCell::new(Vec::new()),
        }
    }

    pub fn path(&self) -> &Path {
        self.temporary.path()
    }

    /// Adds an environment variable for processes started through this
    /// workspace. The language server passes its environment to Spago, so this
    /// also controls the deterministic Spago gate.
    pub fn set_env(&self, key: &str, value: impl Into<String>) {
        self.environment.borrow_mut().push((key.to_owned(), value.into()));
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
        let mut files = Vec::new();
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
            .env("NO_COLOR", "1")
            .env_remove("FORCE_COLOR")
            .env_remove("NODE_OPTIONS");
        for variable in ["TMPDIR", "TMP", "TEMP"] {
            command.env(variable, self.process_temporary.path());
        }
        for (key, value) in self.environment.borrow().iter() {
            command.env(key, value);
        }
        command
    }

    pub fn module_metadata(&self, module: &str) -> Option<serde_json::Value> {
        let path = self.path().join("output").join(module).join("metadata.json");
        let source = fs::read_to_string(path).ok()?;
        serde_json::from_str(&source).ok()
    }

    pub fn metadata_modules(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.path().join("output")) else {
            return Vec::new();
        };
        let directories = entries.map(|entry| entry.unwrap().path());
        let directories = directories.filter(|path| path.join("metadata.json").is_file());
        let modules = directories.map(|path| path.file_name().unwrap().to_string_lossy().into());
        modules.sorted().collect_vec()
    }

    pub fn normalized_metadata(&self) -> String {
        let version = self.command(&["--version"]);
        let version = String::from_utf8_lossy(&version.stdout);
        let version = version.trim_end().strip_prefix("iris ").unwrap();
        let canonical = dunce::canonicalize(self.path()).unwrap();

        let mut summary = String::new();
        for module in self.metadata_modules() {
            let path = format!("output/{module}/metadata.json");
            let mut source = self.read(&path);
            let metadata: serde_json::Value = serde_json::from_str(&source).unwrap();
            assert_eq!(metadata["iris"], version);
            source = source.replace(&format!("\"iris\": \"{version}\""), "\"iris\": \"[VERSION]\"");
            for root in [canonical.as_path(), self.path()] {
                let root = serde_json::to_string(root.to_str().unwrap()).unwrap();
                let root = regex::escape(root.trim_matches('"'));
                let prefix = regex::Regex::new(&format!(r#"{root}(["/]|\\\\)"#)).unwrap();
                source = prefix.replace_all(&source, "[WORKSPACE]$1").into_owned();
            }
            summary.push_str(&format!("--- {path}\n{}", source.replace("\\\\", "/")));
        }
        summary
    }

    pub fn normalize_output(&self, output: &[u8]) -> String {
        let prefixes = workspace_prefixes(self.path());
        let output = String::from_utf8_lossy(output);
        let lines = output.lines().map(|line| {
            let mut line = line.to_owned();
            for prefix in &prefixes {
                line = replace_path_prefix(&line, prefix);
            }
            if line.contains("[WORKSPACE]") {
                line = line.replace('\\', "/");
            }
            normalize_process_status(&line)
        });
        lines.collect_vec().join("\n")
    }

    pub fn workspace_stack_frames(&self, output: &Output) -> String {
        let stderr = self.normalize_output(&output.stderr);
        let frames = stderr.lines().map(str::trim);
        let frames = frames.filter(|line| line.starts_with("at ") && line.contains("[WORKSPACE]"));
        frames.collect_vec().join("\n")
    }

    pub fn report(&self, output: &Output) -> String {
        let status = match output.status.code() {
            Some(code) => code.to_string(),
            None => "signal".to_owned(),
        };
        let mut report = format!("status: {status}\n");
        for (name, stream) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
            report.push_str(&format!("--- {name}\n"));
            for line in self.normalize_output(stream).lines() {
                report.push_str(line);
                report.push('\n');
            }
        }
        report
    }

    pub fn transcript(&self, commands: &[&[&str]]) -> String {
        let reports = commands.iter().map(|arguments| {
            let output = self.command(arguments);
            format!("$ iris {}\n{}", arguments.join(" "), self.report(&output))
        });
        reports.collect()
    }

    pub fn spago_calls(&self) -> String {
        let source = match fs::read_to_string(self.path().join("spago-calls")) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => panic!("failed to read Spago call log: {error}"),
        };
        let root = dunce::canonicalize(self.path()).unwrap();
        let calls = source.lines().map(|line| {
            let mut fields = line.split('\t');
            let directory = dunce::canonicalize(fields.next().unwrap()).unwrap();
            let directory = directory.strip_prefix(&root).unwrap().to_string_lossy();
            let directory = if directory.is_empty() { ".".into() } else { directory };
            let mut arguments = fields.map(|argument| format!("{argument:?}"));
            format!("{}: spago {}", directory.replace('\\', "/"), arguments.join(" "))
        });
        calls.collect_vec().join("\n")
    }
}

pub fn workspace_prefixes(workspace: &Path) -> Vec<String> {
    let mut paths = vec![workspace.to_path_buf()];
    if let Ok(canonical) = dunce::canonicalize(workspace)
        && canonical != workspace
    {
        paths.push(canonical);
    }
    let mut prefixes = paths
        .into_iter()
        .flat_map(|path| {
            let native = path.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
            let slash = native.replace('\\', "/");
            let uri = Url::from_directory_path(&path)
                .expect("invariant violated: workspace path is not absolute")
                .to_string()
                .trim_end_matches('/')
                .to_string();
            [native, slash, uri]
        })
        .collect::<Vec<_>>();
    prefixes
        .sort_unstable_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    prefixes.dedup();
    prefixes
}

pub fn replace_path_prefix(value: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = remaining.find(prefix) {
        let suffix = &remaining[index + prefix.len()..];
        let boundary = suffix.is_empty() || suffix.starts_with(['/', '\\']);
        result.push_str(&remaining[..index]);
        if boundary {
            result.push_str("[WORKSPACE]");
        } else {
            result.push_str(prefix);
        }
        remaining = suffix;
    }
    result.push_str(remaining);
    result
}

pub fn normalize_process_status(text: &str) -> String {
    static STATUS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"exit (?:status|code): (-?\d+)").unwrap());

    STATUS.replace_all(text, "exit code: $1").into_owned()
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

pub fn install_spago_launcher(directory: &Path, executable: &Path) {
    fs::create_dir_all(directory).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(executable, directory.join("spago")).unwrap();
    #[cfg(windows)]
    fs::write(
        directory.join("spago.cmd"),
        format!(
            "@echo off\r\necho used>\"{}\"\r\n\"{}\" %*\r\n",
            directory.join("script-used").display(),
            executable.display(),
        ),
    )
    .unwrap();
}

pub fn install_failing_spago_launcher(directory: &Path) {
    fs::create_dir_all(directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("spago");
        fs::write(&path, "#!/bin/sh\nexit 9\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(windows)]
    fs::write(directory.join("spago.cmd"), "@exit /b 9\r\n").unwrap();
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
