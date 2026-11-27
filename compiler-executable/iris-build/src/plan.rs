//! Pure package dependency planning over normalized source identities.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::slice;

use itertools::Itertools;
use smol_str::SmolStr;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedSource {
    pub path: PathBuf,
    pub identity: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInput {
    pub name: SmolStr,
    pub source_identities: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct PlannedPackage {
    pub name: SmolStr,
    pub source_paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct BuildPlan {
    packages: Vec<PlannedPackage>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BuildPlanError {
    #[error("Spago package '{0}' appears more than once in the build plan")]
    DuplicatePackage(SmolStr),
    #[error("source {path} belongs to both Spago packages {first_package} and {second_package}")]
    ConflictingPackageSource { path: PathBuf, first_package: SmolStr, second_package: SmolStr },
    #[error("Spago selected source {0}, but the lockfile does not assign it to a package")]
    UnownedPackageSource(PathBuf),
}

impl BuildPlan {
    pub fn new(
        selected_sources: Vec<SelectedSource>,
        package_inputs: Vec<PackageInput>,
    ) -> Result<BuildPlan, BuildPlanError> {
        let mut owners: HashMap<PathBuf, SmolStr> = HashMap::new();
        let mut package_names = HashSet::new();
        for package in &package_inputs {
            if !package_names.insert(SmolStr::clone(&package.name)) {
                return Err(BuildPlanError::DuplicatePackage(SmolStr::clone(&package.name)));
            }
            for source_identity in &package.source_identities {
                if let Some(first_package) = owners.get(source_identity) {
                    if first_package != &package.name {
                        return Err(BuildPlanError::ConflictingPackageSource {
                            path: PathBuf::clone(source_identity),
                            first_package: SmolStr::clone(first_package),
                            second_package: SmolStr::clone(&package.name),
                        });
                    }
                } else {
                    owners.insert(PathBuf::clone(source_identity), SmolStr::clone(&package.name));
                }
            }
        }

        let mut selected_by_package: HashMap<SmolStr, Vec<PathBuf>> = HashMap::new();
        for source in selected_sources {
            let Some(owner) = owners.get(&source.identity) else {
                return Err(BuildPlanError::UnownedPackageSource(source.path));
            };
            selected_by_package.entry(SmolStr::clone(owner)).or_default().push(source.path);
        }

        let packages = package_inputs.into_iter().filter_map(|package| {
            let source_paths = selected_by_package.remove(&package.name)?;
            Some(PlannedPackage { source_paths, name: package.name })
        });

        Ok(BuildPlan { packages: packages.collect_vec() })
    }

    pub fn packages(&self) -> slice::Iter<'_, PlannedPackage> {
        self.packages.iter()
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str) -> SelectedSource {
        SelectedSource { path: PathBuf::from(path), identity: PathBuf::from(path) }
    }

    fn package(name: &str, sources: &[&str]) -> PackageInput {
        let source_identities = sources.iter().map(PathBuf::from);
        PackageInput {
            name: SmolStr::new(name),
            source_identities: source_identities.collect_vec(),
        }
    }

    #[test]
    fn source_ownership_must_be_unique() {
        let result = BuildPlan::new(
            vec![source("Shared.purs")],
            vec![package("first", &["Shared.purs"]), package("second", &["Shared.purs"])],
        );

        assert!(matches!(
            result,
            Err(BuildPlanError::ConflictingPackageSource { first_package, second_package, .. })
                if first_package == "first" && second_package == "second"
        ));
    }

    #[test]
    fn package_names_must_be_unique() {
        let result = BuildPlan::new(
            vec![source("First.purs")],
            vec![package("duplicate", &["First.purs"]), package("duplicate", &[])],
        );

        assert_eq!(
            result.unwrap_err(),
            BuildPlanError::DuplicatePackage(SmolStr::new("duplicate"))
        );
    }
}
