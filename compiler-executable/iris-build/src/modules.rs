use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use building::QueryEngine;
use files::FileId;
use itertools::Itertools;
use prim_constants::MODULE_MAP;
use rayon::prelude::*;
use smol_str::SmolStr;

use super::compile::CompileError;
use super::events::{BuildEvent, BuildEventSink};
use super::plan::BuildPlan;
use super::schedule::{self, TaskGraph, TaskId};

const RESOLVE_COST_PER_SOURCE_BYTE: u64 = 1;
const CHECK_COST_PER_SOURCE_BYTE: u64 = 4;
const GENERATE_COST_PER_SOURCE_BYTE: u64 = 2;

#[derive(Clone, Copy)]
enum ModuleTask {
    CheckedCore,
    FunctionalCore,
    Resolve(FileId),
    Check(FileId),
    Generate(FileId),
}

impl ModuleTask {
    fn file_id(self) -> Option<FileId> {
        match self {
            ModuleTask::CheckedCore | ModuleTask::FunctionalCore => None,
            ModuleTask::Resolve(file_id)
            | ModuleTask::Check(file_id)
            | ModuleTask::Generate(file_id) => Some(file_id),
        }
    }

    fn execute(self, engine: &QueryEngine) -> Result<(), CompileError> {
        let engine = engine.snapshot();
        match self {
            ModuleTask::CheckedCore => {
                engine.checked_core()?;
            }
            ModuleTask::FunctionalCore => {
                engine.functional_core()?;
            }
            ModuleTask::Resolve(file_id) => {
                engine.resolved(file_id)?;
                engine.lowered(file_id)?;
                engine.grouped(file_id)?;
                engine.bracketed(file_id)?;
                engine.sectioned(file_id)?;
            }
            ModuleTask::Check(file_id) => {
                engine.checked(file_id)?;
            }
            ModuleTask::Generate(file_id) => {
                engine.foreign_validation(file_id)?;
                let _ = engine.javascript(file_id)?;
            }
        }
        Ok(())
    }
}

struct SurveyedModule {
    file_id: FileId,
    source_bytes: u64,
    imports: Vec<FileId>,
}

fn survey_module(engine: &QueryEngine, file_id: FileId) -> Result<SurveyedModule, CompileError> {
    let engine = engine.snapshot();
    let content = engine.content(file_id)?;
    let indexed = engine.indexed(file_id)?;
    let imports = indexed
        .imports
        .values()
        .filter_map(|import| engine.module_file(import.name.as_deref()?))
        .chain([engine.prim_id()])
        .collect_vec();
    Ok(SurveyedModule { file_id, source_bytes: content.len() as u64, imports })
}

fn survey_modules(
    engine: &QueryEngine,
    file_ids: &[FileId],
) -> Result<Vec<SurveyedModule>, CompileError> {
    let surveyed = file_ids.par_iter().map(|&file_id| survey_module(engine, file_id));
    surveyed.collect()
}

#[derive(Clone, Copy)]
struct FrontendTasks {
    resolve: TaskId,
    check: TaskId,
}

#[derive(Default)]
struct ModuleTasks {
    graph: TaskGraph,
    tasks: Vec<ModuleTask>,
}

impl ModuleTasks {
    fn new(engine: &QueryEngine, sources: &[FileId]) -> Result<ModuleTasks, CompileError> {
        let mut module_tasks = ModuleTasks::default();
        if sources.is_empty() {
            return Ok(module_tasks);
        }

        let surveyed_sources = survey_modules(engine, sources)?;
        let imported = surveyed_sources.iter().flat_map(|module| module.imports.iter().copied());
        let imported = imported.collect::<HashSet<_>>();
        let builtins = MODULE_MAP
            .iter()
            .filter_map(|(name, _)| engine.module_file(name))
            .filter(|builtin| imported.contains(builtin))
            .collect_vec();
        let surveyed_builtins = survey_modules(engine, &builtins)?;
        let surveyed = surveyed_builtins.into_iter().chain(surveyed_sources).collect_vec();
        let modules = surveyed.iter().enumerate().map(|(index, module)| (module.file_id, index));
        let modules = modules.collect::<HashMap<_, _>>();

        let checked_core = module_tasks.add(ModuleTask::CheckedCore, 0);
        let functional_core = module_tasks.add(ModuleTask::FunctionalCore, 0);
        module_tasks.graph.add_dependency(functional_core, checked_core);

        let frontend = surveyed.iter().map(|module| {
            let resolve_cost = module.source_bytes * RESOLVE_COST_PER_SOURCE_BYTE;
            let resolve = module_tasks.add(ModuleTask::Resolve(module.file_id), resolve_cost);
            let check_cost = module.source_bytes * CHECK_COST_PER_SOURCE_BYTE;
            let check = module_tasks.add(ModuleTask::Check(module.file_id), check_cost);
            module_tasks.graph.add_continuation(resolve, check);
            module_tasks.graph.add_dependency(check, checked_core);
            FrontendTasks { resolve, check }
        });
        let frontend = frontend.collect_vec();

        for (module, tasks) in surveyed.iter().zip(&frontend) {
            let imports = module.imports.iter().filter_map(|import| modules.get(import));
            for import in imports {
                module_tasks.graph.add_dependency(tasks.resolve, frontend[*import].resolve);
                module_tasks.graph.add_dependency(tasks.check, frontend[*import].check);
            }
        }
        let (builtin_tasks, source_tasks) = frontend.split_at(builtins.len());
        for tasks in builtin_tasks {
            module_tasks.graph.add_dependency(checked_core, tasks.resolve);
        }
        for (module, tasks) in surveyed[builtins.len()..].iter().zip(source_tasks) {
            let generate_cost = module.source_bytes * GENERATE_COST_PER_SOURCE_BYTE;
            let generate = module_tasks.add(ModuleTask::Generate(module.file_id), generate_cost);
            module_tasks.graph.add_continuation(tasks.check, generate);
            module_tasks.graph.add_dependency(generate, functional_core);
        }

        Ok(module_tasks)
    }

    fn add(&mut self, task: ModuleTask, cost: u64) -> TaskId {
        self.tasks.push(task);
        self.graph.add_task(cost)
    }
}

struct PackageProgress {
    name: SmolStr,
    remaining_modules: AtomicUsize,
    nanoseconds: AtomicU64,
}

struct PackagesProgress<'a> {
    events: &'a dyn BuildEventSink,
    packages: Vec<PackageProgress>,
    owners: HashMap<FileId, usize>,
}

impl<'a> PackagesProgress<'a> {
    fn new(
        plan: &BuildPlan,
        package_sources: &[Vec<FileId>],
        events: &'a dyn BuildEventSink,
    ) -> PackagesProgress<'a> {
        let packages =
            plan.packages().zip(package_sources).map(|(package, sources)| PackageProgress {
                name: SmolStr::clone(&package.name),
                remaining_modules: AtomicUsize::new(sources.len()),
                nanoseconds: AtomicU64::new(0),
            });
        let packages = packages.collect_vec();
        let owners = package_sources
            .iter()
            .enumerate()
            .flat_map(|(index, sources)| sources.iter().map(move |file_id| (*file_id, index)));
        let owners = owners.collect();
        PackagesProgress { events, packages, owners }
    }

    fn record(&self, task: ModuleTask, duration: Duration) {
        let Some(owner) = task.file_id().and_then(|file_id| self.owners.get(&file_id)) else {
            return;
        };
        let package = &self.packages[*owner];
        package.nanoseconds.fetch_add(duration.as_nanos() as u64, Ordering::AcqRel);
        if matches!(task, ModuleTask::Generate(_))
            && package.remaining_modules.fetch_sub(1, Ordering::AcqRel) == 1
        {
            let nanoseconds = package.nanoseconds.load(Ordering::Acquire);
            self.events.send(BuildEvent::PackageCompleted {
                package_name: SmolStr::clone(&package.name),
                duration: Duration::from_nanos(nanoseconds),
            });
        }
    }
}

pub(crate) fn execute(
    engine: &QueryEngine,
    plan: &BuildPlan,
    sources: &HashMap<PathBuf, FileId>,
    events: &dyn BuildEventSink,
) -> Result<(), CompileError> {
    let package_sources = plan.packages().map(|package| {
        let package_sources = package.source_paths.iter().map(|path| sources[path]);
        package_sources.collect_vec()
    });
    let package_sources = package_sources.collect_vec();
    let source_ids = package_sources.iter().flatten().copied().collect_vec();
    let module_tasks = ModuleTasks::new(engine, &source_ids)?;
    let progress = PackagesProgress::new(plan, &package_sources, events);
    schedule::execute_prioritized(&module_tasks.graph, &|task: TaskId| {
        let task = module_tasks.tasks[task.index()];
        let started = Instant::now();
        task.execute(engine)?;
        progress.record(task, started.elapsed());
        Ok(())
    })
}
