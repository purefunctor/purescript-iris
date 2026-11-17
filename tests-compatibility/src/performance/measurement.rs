use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use building::{QueryEngine, prim};
use files::{FileId, Files};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tests_compatibility::allocation::{self, AllocationCounts};

pub const EDITED_MODULE: &str = "Data.Maybe";

const EDIT_STAGE_PREFIX: &str = "edit";

const BODY_EDIT_ORIGINAL: &str = "fromMaybe a = maybe a identity";

const BODY_EDIT_REPLACEMENT: &str = "fromMaybe a = maybe a (\\x -> x)";

pub fn is_edit_stage(stage: &str) -> bool {
    stage.starts_with(EDIT_STAGE_PREFIX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    Callgrind,
    Timing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub preset: String,
    pub package_set: String,
    pub source_files: usize,
    pub source_bytes: usize,
    pub method: Method,
    pub threads: usize,
    pub stages: Vec<StageMeasurement>,
    pub interned: InternedCounts,
    pub peak_resident_bytes: Option<u64>,
    pub output_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageMeasurement {
    pub stage: String,
    pub instructions: Option<u64>,
    pub allocator_instructions: Option<u64>,
    pub allocations: Option<AllocationCounts>,
    pub wall_nanoseconds: Option<u64>,
    pub resident_growth_bytes: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternedCounts {
    pub types: usize,
    pub types_with_unification: usize,
    pub forall_binders: usize,
    pub row_types: usize,
    pub smol_strs: usize,
    pub string_literals: usize,
}

pub struct Corpus {
    pub preset: String,
    pub package_set: String,
    pub sources: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Query {
    Parsed,
    Stabilized,
    Indexed,
    Resolved,
    Lowered,
    Grouped,
    Bracketed,
    Sectioned,
    Checked,
    Functional,
    JavaScript,
    SourceMap,
    Documented,
}

impl Query {
    const ALL: [Query; 13] = [
        Query::Parsed,
        Query::Stabilized,
        Query::Indexed,
        Query::Resolved,
        Query::Lowered,
        Query::Grouped,
        Query::Bracketed,
        Query::Sectioned,
        Query::Checked,
        Query::Functional,
        Query::JavaScript,
        Query::SourceMap,
        Query::Documented,
    ];

    fn name(self) -> &'static str {
        match self {
            Query::Parsed => "parsed",
            Query::Stabilized => "stabilized",
            Query::Indexed => "indexed",
            Query::Resolved => "resolved",
            Query::Lowered => "lowered",
            Query::Grouped => "grouped",
            Query::Bracketed => "bracketed",
            Query::Sectioned => "sectioned",
            Query::Checked => "checked",
            Query::Functional => "functional",
            Query::JavaScript => "javascript",
            Query::SourceMap => "source_map",
            Query::Documented => "documented",
        }
    }

    fn execute(self, engine: &QueryEngine, id: FileId) {
        match self {
            Query::Parsed => drop(black_box(engine.parsed(id))),
            Query::Stabilized => drop(black_box(engine.stabilized(id))),
            Query::Indexed => drop(black_box(engine.indexed(id))),
            Query::Resolved => drop(black_box(engine.resolved(id))),
            Query::Lowered => drop(black_box(engine.lowered(id))),
            Query::Grouped => drop(black_box(engine.grouped(id))),
            Query::Bracketed => drop(black_box(engine.bracketed(id))),
            Query::Sectioned => drop(black_box(engine.sectioned(id))),
            Query::Checked => drop(black_box(engine.checked(id))),
            Query::Functional => drop(black_box(engine.functional(id))),
            Query::JavaScript => drop(black_box(engine.javascript(id))),
            Query::SourceMap => drop(black_box(engine.source_map(id))),
            Query::Documented => drop(black_box(engine.documented(id))),
        }
    }
}

#[derive(Clone, Copy)]
enum Stage<'a> {
    Build(Query),
    Edit(&'a Edit),
}

impl Stage<'_> {
    fn name(self) -> &'static str {
        match self {
            Stage::Build(query) => query.name(),
            Stage::Edit(edit) => edit.name,
        }
    }
}

struct Workload {
    engine: QueryEngine,
    files: Vec<FileId>,
    pool: Option<rayon::ThreadPool>,
}

impl Workload {
    fn for_each_file(&self, query: Query) {
        match &self.pool {
            None => {
                for &id in &self.files {
                    query.execute(&self.engine, id);
                }
            }
            Some(pool) => pool.install(|| {
                self.files.par_iter().for_each(|&id| {
                    let snapshot = self.engine.snapshot();
                    query.execute(&snapshot, id);
                })
            }),
        }
    }

    fn register_module_names(&self) {
        for &id in &self.files {
            if let Ok(content) = self.engine.content(id)
                && let Ok((parsed, _)) = self.engine.parsed(id)
                && let Some(module_name) = parsed.module_name(&content)
            {
                self.engine.insert_module_file(module_name.as_ref(), id);
            }
        }
    }
}

struct Edit {
    name: &'static str,
    file: FileId,
    content: Arc<str>,
}

#[inline(never)]
fn measured_stage(workload: &Workload, stage: Stage) {
    collected_stage(workload, stage)
}

#[inline(never)]
fn collected_stage(workload: &Workload, stage: Stage) {
    match stage {
        Stage::Build(Query::Parsed) => {
            workload.for_each_file(Query::Parsed);
            workload.register_module_names();
        }
        Stage::Build(query) => workload.for_each_file(query),
        Stage::Edit(edit) => {
            workload.engine.set_content(edit.file, Arc::clone(&edit.content));
            for query in Query::ALL {
                workload.for_each_file(query);
            }
        }
    }
}

pub fn measure(corpus: &Corpus, method: Method, threads: usize) -> Result<Measurement> {
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let mut file_ids = files.iter_id().collect::<Vec<_>>();
    file_ids.sort();
    for (uri, content) in &corpus.sources {
        let id = files.insert(uri.as_str(), content.as_str());
        engine.set_content(id, content.as_str());
        file_ids.push(id);
    }

    let pool = if threads == 1 {
        None
    } else {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;
        Some(pool)
    };
    let threads = pool.as_ref().map_or(1, rayon::ThreadPool::current_num_threads);
    let workload = Workload { engine, files: file_ids, pool };

    if method == Method::Callgrind {
        allocation::enable_counting();
    }

    let measure_stage = |stage: Stage| {
        let allocations_before = allocation::counts();
        let resident_before = resident_bytes();
        let start = Instant::now();
        measured_stage(&workload, stage);
        let elapsed = start.elapsed();
        let allocations = allocation::counts().since(allocations_before);
        let resident_after = resident_bytes();

        let stage = stage.name().to_string();
        match method {
            Method::Callgrind => StageMeasurement {
                stage,
                instructions: None,
                allocator_instructions: None,
                allocations: Some(allocations),
                wall_nanoseconds: None,
                resident_growth_bytes: None,
            },
            Method::Timing => StageMeasurement {
                stage,
                instructions: None,
                allocator_instructions: None,
                allocations: None,
                wall_nanoseconds: Some(elapsed.as_nanos() as u64),
                resident_growth_bytes: resident_before
                    .zip(resident_after)
                    .map(|(before, after)| after as i64 - before as i64),
            },
        }
    };

    let mut stage_measurements =
        Query::ALL.map(|query| measure_stage(Stage::Build(query))).to_vec();
    let interned = interned_counts(&workload.engine);
    let output_digest = output_digest_of(&workload);
    let (edited_file, original) = edited_module(&workload)?;
    for edit in edits(edited_file, &original)? {
        stage_measurements.push(measure_stage(Stage::Edit(&edit)));
        workload.engine.set_content(edited_file, Arc::clone(&original));
        for query in Query::ALL {
            workload.for_each_file(query);
        }
    }

    let peak_resident_bytes = match method {
        Method::Callgrind => None,
        Method::Timing => peak_resident_bytes(),
    };

    let measurement = Measurement {
        preset: corpus.preset.clone(),
        package_set: corpus.package_set.clone(),
        source_files: corpus.sources.len(),
        source_bytes: corpus.sources.iter().map(|(_, content)| content.len()).sum(),
        method,
        threads,
        stages: stage_measurements,
        interned,
        peak_resident_bytes,
        output_digest,
    };

    std::mem::forget(workload);
    Ok(measurement)
}

fn edited_module(workload: &Workload) -> Result<(FileId, Arc<str>)> {
    let file = workload.engine.module_file(EDITED_MODULE).with_context(|| {
        format!("the corpus does not contain {EDITED_MODULE}, which the edit stages change")
    })?;
    let original = workload.engine.content(file).map_err(|error| {
        anyhow::anyhow!("failed to read the content of {EDITED_MODULE}: {error:?}")
    })?;
    Ok((file, original))
}

fn edits(file: FileId, original: &str) -> Result<[Edit; 3]> {
    if !original.contains(BODY_EDIT_ORIGINAL) {
        anyhow::bail!(
            "{EDITED_MODULE} does not contain `{BODY_EDIT_ORIGINAL}`, which edit_body changes"
        );
    }
    let edit = |name, content: String| Edit { name, file, content: Arc::from(content) };
    Ok([
        edit("edit_comment", format!("{original}\n-- edited\n")),
        edit("edit_body", original.replacen(BODY_EDIT_ORIGINAL, BODY_EDIT_REPLACEMENT, 1)),
        edit(
            "edit_interface",
            format!("{original}\nirisPerformanceEdit :: Int\nirisPerformanceEdit = 0\n"),
        ),
    ])
}

fn interned_counts(engine: &QueryEngine) -> InternedCounts {
    let statistics = engine.interner_statistics();
    InternedCounts {
        types: statistics.types,
        types_with_unification: statistics.types_with_unification,
        forall_binders: statistics.forall_binders,
        row_types: statistics.row_types,
        smol_strs: statistics.smol_strs,
        string_literals: statistics.string_literals,
    }
}

fn output_digest_of(workload: &Workload) -> String {
    let engine = &workload.engine;
    let mut hasher = DefaultHasher::new();
    for &id in &workload.files {
        match engine.javascript(id) {
            Ok(Ok(module)) => {
                module.name().hash(&mut hasher);
                module.files().index_source().hash(&mut hasher);
                module.files().stylex_theme_source().hash(&mut hasher);
            }
            Ok(Err(error)) => error.to_string().hash(&mut hasher),
            Err(error) => format!("{error:?}").hash(&mut hasher),
        }
        match engine.source_map(id) {
            Ok(Ok(source_map)) => source_map.to_json("index.purs", None).hash(&mut hasher),
            Ok(Err(error)) => error.to_string().hash(&mut hasher),
            Err(error) => format!("{error:?}").hash(&mut hasher),
        }
    }
    format!("{:016x}", hasher.finish())
}

fn resident_bytes() -> Option<u64> {
    process_status_kilobytes("VmRSS:")
}

fn peak_resident_bytes() -> Option<u64> {
    process_status_kilobytes("VmHWM:")
}

fn process_status_kilobytes(field: &str) -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with(field))?;
    let kilobytes = line[field.len()..].trim().trim_end_matches("kB").trim();
    kilobytes.parse::<u64>().ok().map(|kilobytes| kilobytes * 1024)
}
