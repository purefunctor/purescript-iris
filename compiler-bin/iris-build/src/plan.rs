//! Pure package dependency planning over normalized source identities.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::slice;

use itertools::Itertools;
use petgraph::algo::tarjan_scc;
use petgraph::prelude::DiGraphMap;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageId(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageGroupId(usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedSource {
    pub path: PathBuf,
    pub identity: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInput {
    pub name: String,
    pub source_identities: Vec<PathBuf>,
    pub dependencies: Vec<String>,
}

#[derive(Debug)]
pub struct PlannedPackage {
    pub name: String,
    pub source_paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct PackageGroup {
    pub id: PackageGroupId,
    pub packages: Vec<PackageId>,
    pub dependencies: Vec<PackageGroupId>,
    pub dependents: Vec<PackageGroupId>,
}

#[derive(Debug)]
pub struct BuildPlan {
    packages: Vec<PlannedPackage>,
    groups: Vec<PackageGroup>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BuildPlanError {
    #[error("Spago package '{0}' appears more than once in the build plan")]
    DuplicatePackage(String),
    #[error("source {path} belongs to both Spago packages {first_package} and {second_package}")]
    ConflictingPackageSource { path: PathBuf, first_package: String, second_package: String },
    #[error("Spago selected source {0}, but the lockfile does not assign it to a package")]
    UnownedPackageSource(PathBuf),
}

impl BuildPlan {
    pub fn new(
        selected_sources: Vec<SelectedSource>,
        package_inputs: Vec<PackageInput>,
    ) -> Result<BuildPlan, BuildPlanError> {
        let mut owners: HashMap<PathBuf, String> = HashMap::new();
        let mut package_names = HashSet::new();
        for package in &package_inputs {
            if !package_names.insert(String::clone(&package.name)) {
                return Err(BuildPlanError::DuplicatePackage(String::clone(&package.name)));
            }
            for source_identity in &package.source_identities {
                if let Some(first_package) = owners.get(source_identity) {
                    if first_package != &package.name {
                        return Err(BuildPlanError::ConflictingPackageSource {
                            path: PathBuf::clone(source_identity),
                            first_package: String::clone(first_package),
                            second_package: String::clone(&package.name),
                        });
                    }
                } else {
                    owners.insert(PathBuf::clone(source_identity), String::clone(&package.name));
                }
            }
        }

        let mut selected_by_package: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for source in selected_sources {
            let Some(owner) = owners.get(&source.identity) else {
                return Err(BuildPlanError::UnownedPackageSource(source.path));
            };
            selected_by_package.entry(String::clone(owner)).or_default().push(source.path);
        }

        let retained = selected_by_package.keys().cloned().collect::<HashSet<_>>();
        let packages_by_name =
            package_inputs.iter().map(|package| (package.name.as_str(), package));
        let packages_by_name = packages_by_name.collect::<HashMap<_, _>>();
        let contracted_dependencies = package_inputs
            .iter()
            .filter(|package| retained.contains(&package.name))
            .map(|package| {
                let dependencies = contract_dependencies(package, &retained, &packages_by_name);
                (String::clone(&package.name), dependencies)
            });
        let contracted_dependencies = contracted_dependencies.collect::<HashMap<_, _>>();

        let retained_inputs =
            package_inputs.into_iter().filter(|package| retained.contains(&package.name));
        let retained_inputs = retained_inputs.collect_vec();
        let package_ids = retained_inputs
            .iter()
            .enumerate()
            .map(|(index, package)| (package.name.as_str(), PackageId(index)));
        let package_ids = package_ids.collect::<HashMap<_, _>>();
        let package_dependencies = retained_inputs.iter().map(|package| {
            let dependencies = contracted_dependencies[&package.name]
                .iter()
                .filter_map(|dependency| package_ids.get(dependency.as_str()).copied());
            dependencies.collect_vec()
        });
        let package_dependencies = package_dependencies.collect_vec();
        let packages = retained_inputs.into_iter().map(|package| {
            let source_paths = selected_by_package
                .remove(&package.name)
                .expect("invariant violated: retained package has no selected sources");
            PlannedPackage { source_paths, name: package.name }
        });
        let packages = packages.collect_vec();
        let groups = package_groups(&package_dependencies);

        Ok(BuildPlan { packages, groups })
    }

    pub fn packages(&self) -> slice::Iter<'_, PlannedPackage> {
        self.packages.iter()
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    pub fn package(&self, id: PackageId) -> &PlannedPackage {
        &self.packages[id.0]
    }

    pub fn groups(&self) -> slice::Iter<'_, PackageGroup> {
        self.groups.iter()
    }

    pub fn group(&self, id: PackageGroupId) -> &PackageGroup {
        &self.groups[id.0]
    }
}

fn contract_dependencies(
    package: &PackageInput,
    retained: &HashSet<String>,
    packages_by_name: &HashMap<&str, &PackageInput>,
) -> Vec<String> {
    let mut dependencies = vec![];
    let mut visited = HashSet::new();
    let pending = package.dependencies.iter().rev().map(String::as_str);
    let mut pending = pending.collect_vec();
    while let Some(dependency) = pending.pop() {
        if !visited.insert(dependency) {
            continue;
        }
        if retained.contains(dependency) {
            dependencies.push(dependency.to_owned());
        } else if let Some(package) = packages_by_name.get(dependency) {
            pending.extend(package.dependencies.iter().rev().map(String::as_str));
        }
    }
    dependencies
}

impl PackageGroupId {
    pub fn index(self) -> usize {
        self.0
    }
}

fn package_groups(package_dependencies: &[Vec<PackageId>]) -> Vec<PackageGroup> {
    let mut graph = DiGraphMap::<PackageId, ()>::default();
    for index in 0..package_dependencies.len() {
        graph.add_node(PackageId(index));
    }
    for (index, dependencies) in package_dependencies.iter().enumerate() {
        for dependency in dependencies {
            graph.add_edge(PackageId(index), *dependency, ());
        }
    }

    let mut grouped_packages = tarjan_scc(&graph);
    for packages in &mut grouped_packages {
        packages.sort_unstable();
    }
    grouped_packages.sort_unstable_by_key(|packages| packages[0]);

    let mut package_groups = HashMap::new();
    for (group_index, packages) in grouped_packages.iter().enumerate() {
        for package in packages {
            package_groups.insert(*package, PackageGroupId(group_index));
        }
    }

    let dependencies = grouped_packages.iter().enumerate().map(|(group_index, packages)| {
        let group_id = PackageGroupId(group_index);
        let dependencies = packages
            .iter()
            .flat_map(|package| &package_dependencies[package.0])
            .map(|dependency| package_groups[dependency])
            .filter(|dependency| *dependency != group_id)
            .sorted_unstable()
            .dedup();
        dependencies.collect_vec()
    });
    let dependencies = dependencies.collect_vec();
    let mut dependents = vec![vec![]; grouped_packages.len()];
    for (group_index, group_dependencies) in dependencies.iter().enumerate() {
        for dependency in group_dependencies {
            dependents[dependency.0].push(PackageGroupId(group_index));
        }
    }

    let groups = grouped_packages.into_iter().zip(dependencies).zip(dependents).enumerate().map(
        |(index, ((packages, dependencies), dependents))| PackageGroup {
            id: PackageGroupId(index),
            packages,
            dependencies,
            dependents,
        },
    );
    groups.collect_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str) -> SelectedSource {
        SelectedSource { path: PathBuf::from(path), identity: PathBuf::from(path) }
    }

    fn package(name: &str, sources: &[&str], dependencies: &[&str]) -> PackageInput {
        let source_identities = sources.iter().map(PathBuf::from);
        let source_identities = source_identities.collect_vec();
        let dependencies = dependencies.iter().map(|dependency| (*dependency).to_owned());
        PackageInput {
            name: name.to_owned(),
            source_identities,
            dependencies: dependencies.collect_vec(),
        }
    }

    #[test]
    fn source_ownership_must_be_unique() {
        let result = BuildPlan::new(
            vec![source("Shared.purs")],
            vec![package("first", &["Shared.purs"], &[]), package("second", &["Shared.purs"], &[])],
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
            vec![package("duplicate", &["First.purs"], &[]), package("duplicate", &[], &[])],
        );

        assert_eq!(result.unwrap_err(), BuildPlanError::DuplicatePackage("duplicate".to_owned()));
    }

    #[test]
    fn source_empty_packages_preserve_dependency_reachability() {
        let plan = BuildPlan::new(
            vec![source("Library.purs"), source("Application.purs")],
            vec![
                package("library", &["Library.purs"], &[]),
                package("metadata-only", &[], &["library"]),
                package("application", &["Application.purs"], &["metadata-only"]),
            ],
        )
        .unwrap();

        assert_eq!(plan.groups.len(), 2);
        assert!(plan.groups[0].dependencies.is_empty());
        assert_eq!(plan.groups[1].dependencies, [PackageGroupId(0)]);
    }

    #[test]
    fn groups_preserve_dependency_edges_and_independent_roots() {
        let plan = BuildPlan::new(
            vec![source("a"), source("b"), source("middle"), source("leaf")],
            vec![
                package("root-a", &["a"], &[]),
                package("root-b", &["b"], &[]),
                package("middle", &["middle"], &["root-a"]),
                package("leaf", &["leaf"], &["root-b", "middle"]),
            ],
        )
        .unwrap();

        assert_eq!(plan.groups.len(), 4);
        assert!(plan.groups[0].dependencies.is_empty());
        assert!(plan.groups[1].dependencies.is_empty());
        assert_eq!(plan.groups[2].dependencies, [PackageGroupId(0)]);
        assert_eq!(plan.groups[3].dependencies, [PackageGroupId(1), PackageGroupId(2)]);
    }

    #[test]
    fn cycles_are_one_scheduling_group() {
        let plan = BuildPlan::new(
            vec![source("root"), source("cycle-a"), source("cycle-b"), source("downstream")],
            vec![
                package("root", &["root"], &[]),
                package("cycle-a", &["cycle-a"], &["root", "cycle-b"]),
                package("cycle-b", &["cycle-b"], &["cycle-a"]),
                package("downstream", &["downstream"], &["cycle-b"]),
            ],
        )
        .unwrap();

        assert_eq!(plan.groups.len(), 3);
        assert_eq!(plan.groups[1].packages, [PackageId(1), PackageId(2)]);
        assert_eq!(plan.groups[1].dependencies, [PackageGroupId(0)]);
        assert_eq!(plan.groups[2].dependencies, [PackageGroupId(1)]);
    }
}
