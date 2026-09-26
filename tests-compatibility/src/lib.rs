use std::collections::HashSet;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use building::{QueryEngine, prim};
use clap::ValueEnum;
use files::{FileId, Files};
use globset::Glob;
use rayon::prelude::*;
use walkdir::WalkDir;

pub mod package_cache;
pub mod registry;

pub use package_cache::PackageCache;

// Match the allocator shipped by the `iris` binary so that verification and
// benchmarks observe the same multi-threaded allocation behaviour.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const CORE_PACKAGES: &str = include_str!("../purescript-core.txt");
const ACME_PACKAGES: &str = include_str!("../purescript-acme.txt");

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq, PartialOrd, Ord)]
pub enum Preset {
    Core,
    Acme,
}

impl Preset {
    pub fn packages_text(self) -> &'static str {
        match self {
            Preset::Core => CORE_PACKAGES,
            Preset::Acme => ACME_PACKAGES,
        }
    }
}

pub fn default_cache_dir() -> PathBuf {
    workspace_root().join("target/compatibility")
}

pub fn all_source_files() -> Vec<PathBuf> {
    let mut source_files = cache_source_files_for(None);
    source_files.sort();
    source_files.dedup();
    source_files
}

pub fn benchmark_sources() -> (&'static str, Arc<[String]>) {
    let mut paths = all_source_files();
    let corpus = if paths.is_empty() {
        paths.extend(find_purs_files(workspace_root().join("tests-integration/fixtures")));
        paths.sort();
        paths.dedup();
        "integration-fixtures"
    } else {
        "compatibility-cache"
    };

    let sources = paths
        .iter()
        .map(|path| {
            fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        })
        .collect::<Arc<[_]>>();
    assert!(!sources.is_empty(), "benchmark corpus is empty");

    (corpus, sources)
}

fn package_purs_files(pkg_dir: &Path) -> impl Iterator<Item = PathBuf> {
    [pkg_dir.join("src"), pkg_dir.join("test")].into_iter().flat_map(find_purs_files)
}

fn find_purs_files(root: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    let matcher = Glob::new("**/*.purs").unwrap().compile_matcher();
    WalkDir::new(root).into_iter().filter_map(move |entry| {
        let entry = entry.ok()?;
        let path = entry.path();
        if matcher.is_match(path) { Some(path.to_path_buf()) } else { None }
    })
}

pub fn core_source_files() -> Vec<PathBuf> {
    source_files_for(&parse_package_list(CORE_PACKAGES))
}

pub fn acme_source_files() -> Vec<PathBuf> {
    source_files_for(&parse_package_list(ACME_PACKAGES))
}

pub fn preset_package_names(presets: &[Preset]) -> HashSet<String> {
    presets.iter().flat_map(|preset| parse_package_list(preset.packages_text())).collect()
}

pub fn parse_package_list(text: &str) -> HashSet<String> {
    let names = text.lines().filter(|name| !name.is_empty()).map(str::to_owned);
    names.collect()
}

fn source_files_for(names: &HashSet<String>) -> Vec<PathBuf> {
    let mut source_files = cache_source_files_for(Some(names));
    source_files.sort();
    source_files.dedup();
    source_files
}

fn cache_source_files_for(names: Option<&HashSet<String>>) -> Vec<PathBuf> {
    let sources = default_cache_dir().join("sources");

    let mut source_files = Vec::new();
    let package_entries = match fs::read_dir(&sources) {
        Ok(entries) => entries,
        Err(_) => return source_files,
    };

    for package_entry in package_entries.flatten() {
        if !package_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let package_name = package_entry.file_name().to_string_lossy().into_owned();
        if names.is_some_and(|names| !names.contains(package_name.as_str())) {
            continue;
        }

        let Ok(version_entries) = fs::read_dir(package_entry.path()) else { continue };
        for version_entry in version_entries.flatten() {
            if !version_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            source_files.extend(package_purs_files(&version_entry.path()));
        }
    }

    source_files
}

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    manifest_dir().parent().expect("tests-compatibility is under workspace root").to_path_buf()
}

pub fn load_sources(paths: Vec<PathBuf>) -> Arc<[(String, String)]> {
    let sources = paths.iter().filter_map(|path| {
        let content = fs::read_to_string(path).ok()?;
        let uri = path.to_string_lossy().into_owned();
        Some((uri, content))
    });
    sources.collect()
}

pub struct WarmedEngine {
    pub engine: QueryEngine,
    pub candidates: Vec<FileId>,
}

pub fn build_warmed_engine(sources: &[(String, String)]) -> WarmedEngine {
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let mut candidates = Vec::new();
    for (uri, content) in sources {
        let file_id = files.insert(uri.as_str(), content.as_str());
        engine.set_content(file_id, content.as_str());
        candidates.push(file_id);
    }

    for &file_id in &candidates {
        if let Ok(content) = engine.content(file_id)
            && let Ok((parsed, _)) = engine.parsed(file_id)
            && let Some(module_name) = parsed.module_name(&content)
        {
            engine.set_module_file(module_name.as_ref(), file_id);
        }
    }

    candidates.par_iter().for_each(|&file_id| {
        let snapshot = engine.snapshot();
        let _ = snapshot.parsed(file_id);
        let _ = snapshot.stabilized(file_id);
        let _ = snapshot.indexed(file_id);
        let _ = snapshot.resolved(file_id);
        let _ = snapshot.lowered(file_id);
        let _ = snapshot.grouped(file_id);
        let _ = snapshot.bracketed(file_id);
        let _ = snapshot.sectioned(file_id);
    });

    WarmedEngine { engine, candidates }
}

pub fn run_single_core(warmed: &WarmedEngine) {
    for &file_id in &warmed.candidates {
        let _ = black_box(warmed.engine.checked(file_id));
    }
}

pub fn run_multi_core(warmed: &WarmedEngine) {
    let engine = &warmed.engine;
    warmed.candidates.par_iter().for_each(|&file_id| {
        let snapshot = engine.snapshot();
        let _ = black_box(snapshot.checked(file_id));
    });
}

pub fn probe_iter_time(
    sources: &[(String, String)],
    routine: impl FnOnce(&WarmedEngine),
) -> Duration {
    let warmed = build_warmed_engine(sources);
    let start = Instant::now();
    routine(&warmed);
    start.elapsed()
}
