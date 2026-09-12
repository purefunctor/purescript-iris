use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus};
use std::{env, fs, io};

use itertools::Itertools;
use thiserror::Error;
use url::Url;

use crate::cli::ColorChoice;
use crate::compile::{self, CompileError, Resilience};
use crate::watch::{self, WatchError};
use crate::workspace::{Workspace, WorkspaceError};
use spago::{SpagoCommand, SpagoError};

const MAIN_SOURCE: &str = include_str!("../bundled/project/Main.purs");
const TEST_SOURCE: &str = include_str!("../bundled/project/Test.Main.purs");
const NODE_RUNNER: &str = include_str!("../bundled/project/runner.mjs");
const GITIGNORE: &str = include_str!("../bundled/project/gitignore");

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Spago(#[from] SpagoError),
    #[error(transparent)]
    SpagoLock(#[from] spago::LockfileGlobSetError),
    #[error(transparent)]
    Watch(#[from] WatchError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("invalid package name '{0}'; use lowercase letters, digits, and hyphens")]
    InvalidPackageName(String),
    #[error("cannot create a project because these paths already exist: {0}")]
    ExistingPaths(String),
    #[error("cannot create a project inside the existing Spago workspace at {0}")]
    ExistingWorkspace(PathBuf),
    #[error("failed to determine the current directory: {0}")]
    CurrentDirectory(io::Error),
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to execute Node.js: {0}")]
    Node(io::Error),
    #[error("Node.js exited without a status code")]
    MissingStatus,
    #[error("Node.js exited with status {0}")]
    NodeFailed(ExitStatus),
    #[error("module output does not exist: {0}")]
    MissingModule(PathBuf),
    #[error("failed to canonicalize module output {path}: {source}")]
    CanonicalizeModule {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to convert module output to a file URL: {0}")]
    ModuleUrl(PathBuf),
    #[error("no selected packages contain tests")]
    NoTests,
}

pub struct BuildProjectConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub quiet: bool,
    pub color: ColorChoice,
    pub diagnostics: bool,
}

pub struct AddProjectConfig {
    pub package: Option<String>,
    pub dependencies: Vec<String>,
    pub test_dependencies: bool,
}

pub struct RunProjectConfig {
    pub build: BuildProjectConfig,
    pub main: Option<String>,
    pub arguments: Vec<String>,
}

pub struct TestProjectConfig {
    pub build: BuildProjectConfig,
    pub main: Option<String>,
    pub arguments: Vec<String>,
}

pub fn start(result: Result<(), ProjectError>) {
    if let Err(error) = result {
        if !matches!(&error, ProjectError::Compile(CompileError::Diagnostics { reported: false })) {
            eprintln!("{error}");
        }
        let exit_code = match error {
            ProjectError::NodeFailed(status) => status.code().unwrap_or(1),
            _ => 1,
        };
        process::exit(exit_code);
    }
}

pub fn new(name: Option<String>) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    if let Some(root) = Workspace::find_ancestor(&current_directory)? {
        return Err(ProjectError::ExistingWorkspace(root));
    }
    let name = match name {
        Some(name) => name,
        None => current_directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("main")
            .to_owned(),
    };
    validate_package_name(&name)?;

    let targets = [
        current_directory.join("spago.yaml"),
        current_directory.join("src/Main.purs"),
        current_directory.join("test/Test/Main.purs"),
        current_directory.join(".gitignore"),
    ];
    let required_directories = [
        current_directory.join("src"),
        current_directory.join("test"),
        current_directory.join("test/Test"),
    ];
    let existing_targets = targets.iter().filter(|path| path.exists());
    let invalid_directories =
        required_directories.iter().filter(|path| path.exists() && !path.is_dir());
    let existing = existing_targets.chain(invalid_directories);
    let existing = existing.map(|path| path.display().to_string());
    let existing = existing.collect::<Vec<_>>();
    if !existing.is_empty() {
        return Err(ProjectError::ExistingPaths(existing.join(", ")));
    }

    let manifest = format!(
        r#"package:
  name: {name}
  dependencies:
    - console
    - effect
    - prelude
  test:
    main: Test.Main
    dependencies:
      - assert
workspace: {{}}
"#
    );
    write_file(&targets[0], &manifest)?;
    write_file(&targets[1], MAIN_SOURCE)?;
    write_file(&targets[2], TEST_SOURCE)?;
    write_file(&targets[3], GITIGNORE)?;
    Ok(())
}

pub fn build(config: BuildProjectConfig, resilience: Resilience) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    compile_workspace(&workspace, &current_directory, &config, resilience)
}

pub fn watch(config: BuildProjectConfig) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    let (inputs, output) = prepare_workspace_build(&workspace, &current_directory, &config)?;
    watch::watch(watch::WatchConfig {
        current_directory: workspace.root,
        output,
        inputs,
        quiet: config.quiet,
        color: config.color,
    })?;
    Ok(())
}

pub fn add(config: AddProjectConfig) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    let selected = workspace.require_selected()?.manifest.name.as_str();
    let spago = SpagoCommand::new(&current_directory)?;
    spago.add(selected, &config.dependencies, config.test_dependencies).map_err(ProjectError::from)
}

pub fn run(config: RunProjectConfig) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.build.package.as_deref())?;
    let package = workspace.require_selected()?;
    compile_workspace(&workspace, &current_directory, &config.build, Resilience::Strict)?;

    let execution = package.manifest.run.as_ref().cloned().unwrap_or_default();
    let main = config.main.or(execution.main).unwrap_or_else(|| "Main".to_owned());
    let arguments =
        if config.arguments.is_empty() { execution.exec_args } else { config.arguments };
    execute_module(&workspace.root, output(&workspace, &config.build), &main, &arguments)
}

pub fn test(config: TestProjectConfig) -> Result<(), ProjectError> {
    let current_directory = env::current_dir().map_err(ProjectError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.build.package.as_deref())?;
    if workspace.selected.is_some() {
        let package = workspace.require_selected()?;
        if !package.has_tests {
            return Err(ProjectError::NoTests);
        }
    } else if !workspace.packages.values().any(|package| package.has_tests) {
        return Err(ProjectError::NoTests);
    }
    compile_workspace(&workspace, &current_directory, &config.build, Resilience::Strict)?;
    let output = output(&workspace, &config.build);

    let packages = workspace.packages.values().filter(|package| {
        workspace.selected.as_ref().is_some_and(|selected| selected == &package.manifest.name)
            || (workspace.selected.is_none() && package.has_tests)
    });
    for package in packages {
        let execution = package.manifest.test.as_ref().cloned().unwrap_or_default();
        let main = config.main.clone().or(execution.main).unwrap_or_else(|| "Test.Main".to_owned());
        let arguments = if config.arguments.is_empty() {
            execution.exec_args
        } else {
            config.arguments.clone()
        };
        execute_module(&workspace.root, output.clone(), &main, &arguments)?;
    }
    Ok(())
}

fn compile_workspace(
    workspace: &Workspace,
    current_directory: &Path,
    config: &BuildProjectConfig,
    resilience: Resilience,
) -> Result<(), ProjectError> {
    let (sources, output) = prepare_workspace_build(workspace, current_directory, config)?;
    let packages = spago::source_files_by_package(&workspace.root)?;
    let packages = packages.into_iter().map(|(name, package)| compile::PackageInput {
        name: name.to_string(),
        sources: package.sources,
        dependencies: package.dependencies.into_iter().map(|name| name.to_string()).collect_vec(),
    });
    let packages = packages.collect_vec();
    compile::compile_package_inputs(
        &workspace.root,
        &output,
        &sources,
        packages,
        config.quiet,
        config.color,
        config.diagnostics,
        resilience,
    )?;
    Ok(())
}

fn prepare_workspace_build(
    workspace: &Workspace,
    current_directory: &Path,
    config: &BuildProjectConfig,
) -> Result<(Vec<PathBuf>, PathBuf), ProjectError> {
    let spago = SpagoCommand::new(current_directory)?;
    spago.fetch(workspace.selected.as_deref(), !config.quiet)?;
    let sources = spago.source_globs(workspace.selected.as_deref(), !config.quiet)?;
    let output = output(workspace, config);
    Ok((sources, output))
}

fn output(workspace: &Workspace, config: &BuildProjectConfig) -> PathBuf {
    config.output.clone().unwrap_or_else(|| workspace.root.join("output"))
}

fn execute_module(
    workspace_root: &Path,
    output: PathBuf,
    module: &str,
    arguments: &[String],
) -> Result<(), ProjectError> {
    let module = output.join(module).join("index.js");
    if !module.is_file() {
        return Err(ProjectError::MissingModule(module));
    }
    let module = module
        .canonicalize()
        .map_err(|source| ProjectError::CanonicalizeModule { path: module.clone(), source })?;
    let module_url = Url::from_file_path(&module)
        .map_err(|()| ProjectError::ModuleUrl(module.clone()))?
        .to_string();
    let status = Command::new("node")
        .arg("--input-type=module")
        .arg("--eval")
        .arg(NODE_RUNNER)
        .arg(module_url)
        .arg("--")
        .args(arguments)
        .current_dir(workspace_root)
        .status()
        .map_err(ProjectError::Node)?;
    ensure_node_success(status)
}

fn ensure_node_success(status: ExitStatus) -> Result<(), ProjectError> {
    if status.success() {
        return Ok(());
    }
    if status.code().is_none() {
        return Err(ProjectError::MissingStatus);
    }
    Err(ProjectError::NodeFailed(status))
}

fn validate_package_name(name: &str) -> Result<(), ProjectError> {
    let mut characters = name.chars();
    let starts_lowercase =
        characters.next().is_some_and(|character| character.is_ascii_lowercase());
    let remaining_valid = characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    });
    if !starts_lowercase || !remaining_valid {
        return Err(ProjectError::InvalidPackageName(name.to_owned()));
    }
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), ProjectError> {
    let parent = path.parent().expect("project file path has no parent");
    fs::create_dir_all(parent)
        .map_err(|source| ProjectError::Write { path: parent.to_path_buf(), source })?;
    fs::write(path, content)
        .map_err(|source| ProjectError::Write { path: path.to_path_buf(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_package_names() {
        assert!(validate_package_name("my-project2").is_ok());
        assert!(validate_package_name("MyProject").is_err());
        assert!(validate_package_name("2project").is_err());
    }
}
