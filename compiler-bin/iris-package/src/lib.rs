use std::path::{Path, PathBuf};
use std::{env, fs, io};

use itertools::Itertools;
use spago::{SpagoCommand, SpagoError};
use thiserror::Error;

use crate::workspace::{Workspace, WorkspaceError};

mod workspace;

const MAIN_SOURCE: &str = include_str!("../bundled/project/Main.purs");
const TEST_SOURCE: &str = include_str!("../bundled/project/Test.Main.purs");
const GITIGNORE: &str = include_str!("../bundled/project/gitignore");

pub struct NewConfig {
    pub name: Option<String>,
}

pub struct AddConfig {
    pub package: Option<String>,
    pub dependencies: Vec<String>,
    pub test_dependencies: bool,
}

#[derive(Debug, Error)]
pub enum PackageManagerError {
    #[error(transparent)]
    Spago(#[from] SpagoError),
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
}

pub fn create(config: NewConfig) -> Result<(), PackageManagerError> {
    let current_directory = env::current_dir().map_err(PackageManagerError::CurrentDirectory)?;
    if let Some(root) = Workspace::find_ancestor(&current_directory)? {
        return Err(PackageManagerError::ExistingWorkspace(root));
    }
    let name = config.name.unwrap_or_else(|| {
        current_directory.file_name().and_then(|name| name.to_str()).unwrap_or("main").to_owned()
    });
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
    let existing = existing.map(|path| path.display().to_string()).collect_vec();
    if !existing.is_empty() {
        return Err(PackageManagerError::ExistingPaths(existing.join(", ")));
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
    write_file(&targets[3], GITIGNORE)
}

pub fn add(config: AddConfig) -> Result<(), PackageManagerError> {
    let current_directory = env::current_dir().map_err(PackageManagerError::CurrentDirectory)?;
    let workspace = Workspace::discover(&current_directory, config.package.as_deref())?;
    let selected = workspace.require_selected()?;
    let spago = SpagoCommand::new(&current_directory)?;
    spago
        .add(selected, &config.dependencies, config.test_dependencies)
        .map_err(PackageManagerError::from)
}

fn validate_package_name(name: &str) -> Result<(), PackageManagerError> {
    let mut characters = name.chars();
    let starts_lowercase =
        characters.next().is_some_and(|character| character.is_ascii_lowercase());
    let remaining_valid = characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    });
    if !starts_lowercase || !remaining_valid {
        return Err(PackageManagerError::InvalidPackageName(name.to_owned()));
    }
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), PackageManagerError> {
    let parent = path.parent().expect("invariant violated: project file path has no parent");
    fs::create_dir_all(parent)
        .map_err(|source| PackageManagerError::Write { path: parent.to_path_buf(), source })?;
    fs::write(path, content)
        .map_err(|source| PackageManagerError::Write { path: path.to_path_buf(), source })
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
