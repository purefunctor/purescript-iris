use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use itertools::Itertools;
use rayon::prelude::*;

use super::events::{BuildEvent, BuildEventSink};
use super::plan::{BuildPlan, PackageGroup, PackageGroupId, PlannedPackage};

struct ParallelExecutor<'a, E, F, S: ?Sized> {
    plan: &'a BuildPlan,
    remaining_dependencies: Vec<AtomicUsize>,
    events: &'a S,
    execute: &'a F,
    failed: AtomicBool,
    error: Mutex<Option<E>>,
}

impl<'a, E, F, S> ParallelExecutor<'a, E, F, S>
where
    E: Send,
    F: Fn(&PlannedPackage) -> Result<(), E> + Sync,
    S: BuildEventSink + ?Sized,
{
    fn spawn<'scope>(&'scope self, scope: &rayon::Scope<'scope>, group_id: PackageGroupId) {
        scope.spawn(move |scope| self.execute_group(scope, group_id));
    }

    fn execute_group<'scope>(&'scope self, scope: &rayon::Scope<'scope>, group_id: PackageGroupId) {
        let group = self.plan.group(group_id);
        let result = group.packages.par_iter().try_for_each(|package_id| {
            execute_package(self.plan.package(*package_id), self.events, self.execute)
        });
        if let Err(package_error) = result {
            self.failed.store(true, Ordering::Release);
            let mut error =
                self.error.lock().expect("invariant violated: package build error is not poisoned");
            error.get_or_insert(package_error);
            return;
        }
        if self.failed.load(Ordering::Acquire) {
            return;
        }
        for dependent in &group.dependents {
            let index = dependent.index();
            if self.remaining_dependencies[index].fetch_sub(1, Ordering::AcqRel) == 1 {
                self.spawn(scope, *dependent);
            }
        }
    }
}

pub fn execute_parallel<E, F, S>(plan: &BuildPlan, events: &S, execute: &F) -> Result<(), E>
where
    E: Send,
    F: Fn(&PlannedPackage) -> Result<(), E> + Sync,
    S: BuildEventSink + ?Sized,
{
    let remaining_dependencies =
        plan.groups().map(|group| AtomicUsize::new(group.dependencies.len()));
    let remaining_dependencies = remaining_dependencies.collect_vec();
    let executor = ParallelExecutor {
        plan,
        remaining_dependencies,
        events,
        execute,
        failed: AtomicBool::new(false),
        error: Mutex::new(None),
    };

    rayon::scope(|scope| {
        for group in executor.plan.groups() {
            if group.dependencies.is_empty() {
                executor.spawn(scope, group.id);
            }
        }
    });

    executor
        .error
        .into_inner()
        .expect("invariant violated: package build error is not poisoned")
        .map_or(Ok(()), Err)
}

pub fn execute_serial<E, F, S>(plan: &BuildPlan, events: &S, execute: &F) -> Result<(), E>
where
    F: Fn(&PlannedPackage) -> Result<(), E>,
    S: BuildEventSink + ?Sized,
{
    let mut completed_groups = vec![false; plan.groups().len()];
    while completed_groups.iter().any(|completed| !completed) {
        let group = plan
            .groups()
            .find(|group| group_is_ready(group, &completed_groups))
            .expect("invariant violated: build plan contains an unschedulable condensed graph");
        for package_id in &group.packages {
            let package = plan.package(*package_id);
            execute_package(package, events, execute)?;
        }
        let index = group.id.index();
        completed_groups[index] = true;
    }
    Ok(())
}

fn execute_package<E, F, S>(package: &PlannedPackage, events: &S, execute: &F) -> Result<(), E>
where
    F: Fn(&PlannedPackage) -> Result<(), E> + ?Sized,
    S: BuildEventSink + ?Sized,
{
    let started = Instant::now();
    execute(package)?;
    events.send(BuildEvent::PackageCompleted {
        package_name: String::clone(&package.name),
        duration: started.elapsed(),
    });
    Ok(())
}

fn group_is_ready(group: &PackageGroup, completed_groups: &[bool]) -> bool {
    let dependencies_complete = group.dependencies.iter().all(|dependency| {
        let index = dependency.index();
        completed_groups[index]
    });
    let index = group.id.index();
    !completed_groups[index] && dependencies_complete
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use itertools::Itertools;

    use super::*;
    use crate::events::{BuildEvent, RecordedBuildEvents};
    use crate::plan::{PackageInput, SelectedSource};

    fn plan() -> BuildPlan {
        let selected_sources = ["root", "independent", "dependent"].into_iter().map(|path| {
            SelectedSource { path: PathBuf::from(path), identity: PathBuf::from(path) }
        });
        let selected_sources = selected_sources.collect_vec();
        let packages = vec![
            PackageInput {
                name: "root".to_owned(),
                source_identities: vec![PathBuf::from("root")],
                dependencies: vec![],
            },
            PackageInput {
                name: "independent".to_owned(),
                source_identities: vec![PathBuf::from("independent")],
                dependencies: vec![],
            },
            PackageInput {
                name: "dependent".to_owned(),
                source_identities: vec![PathBuf::from("dependent")],
                dependencies: vec!["root".to_owned(), "independent".to_owned()],
            },
        ];
        BuildPlan::new(selected_sources, packages).unwrap()
    }

    #[test]
    fn parallel_execution_runs_each_package_once_after_its_dependencies() {
        let plan = plan();
        let events = RecordedBuildEvents::default();
        let root_completed = AtomicBool::new(false);
        let independent_completed = AtomicBool::new(false);
        let executed = Mutex::new(vec![]);

        execute_parallel(&plan, &events, &|package| {
            if package.name == "dependent" {
                assert!(root_completed.load(Ordering::Acquire));
                assert!(independent_completed.load(Ordering::Acquire));
            }
            if package.name == "root" {
                root_completed.store(true, Ordering::Release);
            }
            if package.name == "independent" {
                independent_completed.store(true, Ordering::Release);
            }
            executed.lock().unwrap().push(String::clone(&package.name));
            Ok::<_, ()>(())
        })
        .unwrap();

        let executed = executed.into_inner().unwrap();
        assert_eq!(executed.len(), 3);
        let executed = executed.into_iter().collect::<HashSet<_>>();
        assert_eq!(
            executed,
            HashSet::from(["root".to_owned(), "independent".to_owned(), "dependent".to_owned()])
        );
        let completions = events.into_events().into_iter().filter_map(|event| match event {
            BuildEvent::PackageCompleted { package_name, .. } => Some(package_name),
            _ => None,
        });
        let completions = completions.collect_vec();
        assert_eq!(completions.len(), 3);
        assert_eq!(completions.into_iter().collect::<HashSet<_>>(), executed);
    }

    #[test]
    fn serial_execution_obeys_the_same_plan_and_emits_the_same_packages() {
        let plan = plan();
        let recorded_events = RecordedBuildEvents::default();
        let events: &dyn BuildEventSink = &recorded_events;
        let executed = Mutex::new(vec![]);

        execute_serial(&plan, events, &|package| {
            executed.lock().unwrap().push(String::clone(&package.name));
            Ok::<_, ()>(())
        })
        .unwrap();

        assert_eq!(executed.into_inner().unwrap(), ["root", "independent", "dependent"]);
        assert_eq!(recorded_events.into_events().len(), 3);
    }

    #[test]
    fn failed_dependencies_do_not_release_dependents() {
        let plan = plan();
        let events = RecordedBuildEvents::default();
        let dependent_executed = AtomicBool::new(false);

        let result = execute_parallel(&plan, &events, &|package| {
            if package.name == "root" {
                return Err("root failed");
            }
            if package.name == "dependent" {
                dependent_executed.store(true, Ordering::Release);
            }
            Ok(())
        });

        assert_eq!(result, Err("root failed"));
        assert!(!dependent_executed.load(Ordering::Acquire));
    }
}
