use std::time::Instant;

use smol_str::SmolStr;

use super::events::{BuildEvent, BuildEventSink};
use super::plan::{BuildPlan, PackageGroup, PlannedPackage};

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
        package_name: SmolStr::clone(&package.name),
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
    use std::path::PathBuf;
    use std::sync::Mutex;

    use itertools::Itertools;

    use super::*;
    use crate::events::RecordedBuildEvents;
    use crate::plan::{PackageInput, SelectedSource};

    fn plan() -> BuildPlan {
        let selected_sources = ["root", "independent", "dependent"].into_iter().map(|path| {
            SelectedSource { path: PathBuf::from(path), identity: PathBuf::from(path) }
        });
        let selected_sources = selected_sources.collect_vec();
        let packages = vec![
            PackageInput {
                name: SmolStr::new("root"),
                source_identities: vec![PathBuf::from("root")],
                dependencies: Vec::new(),
            },
            PackageInput {
                name: SmolStr::new("independent"),
                source_identities: vec![PathBuf::from("independent")],
                dependencies: Vec::new(),
            },
            PackageInput {
                name: SmolStr::new("dependent"),
                source_identities: vec![PathBuf::from("dependent")],
                dependencies: vec![SmolStr::new("root"), SmolStr::new("independent")],
            },
        ];
        BuildPlan::new(selected_sources, packages).unwrap()
    }

    #[test]
    fn serial_execution_obeys_the_same_plan_and_emits_the_same_packages() {
        let plan = plan();
        let recorded_events = RecordedBuildEvents::default();
        let events: &dyn BuildEventSink = &recorded_events;
        let executed = Mutex::new(Vec::new());

        execute_serial(&plan, events, &|package| {
            executed.lock().unwrap().push(SmolStr::clone(&package.name));
            Ok::<_, ()>(())
        })
        .unwrap();

        assert_eq!(executed.into_inner().unwrap(), ["root", "independent", "dependent"]);
        assert_eq!(recorded_events.into_events().len(), 3);
    }
}
