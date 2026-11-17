pub mod callgrind;
pub mod comparison;
pub mod measurement;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use rayon::prelude::*;
use tests_compatibility::registry::{FsRegistry, RegistryLayout, RegistryReader};
use tests_compatibility::{PackageCache, default_cache_dir};

use crate::verifier::cli::{
    ComparePerformanceArgs, DEFAULT_INDEX_DIR, DEFAULT_REGISTRY_DIR, MeasureArgs,
};
use crate::verifier::selection::{package_map, resolve_selection};
use crate::verifier::sources::discover_sources;
use crate::{ensure_default_clone, prepare_packages};

use self::measurement::{Corpus, Measurement, Method};

pub fn measure(args: MeasureArgs) -> Result<()> {
    let corpus = load_corpus(&args)?;

    let measurement = if args.timing {
        measurement::measure(&corpus, Method::Timing, args.threads)?
    } else if args.callgrind_child {
        measurement::measure(&corpus, Method::Callgrind, 1)?
    } else {
        measure_with_callgrind(&args, &corpus)?
    };

    write_measurement(&args.json_output, &measurement)
}

fn measure_with_callgrind(args: &MeasureArgs, corpus: &Corpus) -> Result<Measurement> {
    let output_dir = args.json_output.with_extension("callgrind");
    let child_output = output_dir.join("measurement.json");
    let cache_dir = args.cache_dir.clone().unwrap_or_else(default_cache_dir);

    let mut child_arguments: Vec<OsString> = vec!["measure".into()];
    let options = [
        ("--registry-dir", absolute(&args.registry_dir)?),
        ("--index-dir", absolute(&args.index_dir)?),
        ("--cache-dir", absolute(&cache_dir)?),
        ("--json-output", absolute(&child_output)?),
    ];
    for (option, value) in options {
        child_arguments.push(option.into());
        child_arguments.push(value.into());
    }
    child_arguments.extend(["--package-set".into(), corpus.package_set.clone().into()]);
    child_arguments.extend(["--preset".into(), corpus.preset.clone().into()]);
    child_arguments.push("--callgrind-child".into());

    let dumps = callgrind::run(&output_dir, &child_arguments)?;
    let mut measurement = read_measurement(&child_output)?;
    if dumps.len() != measurement.stages.len() {
        bail!(
            "Callgrind wrote {} stage dumps for {} stages; inspect {}",
            dumps.len(),
            measurement.stages.len(),
            output_dir.display()
        );
    }
    for (stage, dump) in measurement.stages.iter_mut().zip(dumps) {
        let allocations = stage.allocations.map_or(0, |allocations| allocations.allocations);
        if allocations > 0 && dump.allocator_instructions == 0 {
            bail!(
                "the {} stage made {allocations} allocations, but Callgrind attributed no \
                 instructions to mimalloc's entry points; the allocator split is broken",
                stage.stage
            );
        }
        stage.instructions = Some(dump.instructions);
        stage.allocator_instructions = Some(dump.allocator_instructions);
        let stage_path = output_dir.join(format!("{}.callgrind.out", stage.stage));
        fs::rename(&dump.path, &stage_path)
            .with_context(|| format!("failed to rename {}", dump.path.display()))?;
    }
    Ok(measurement)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).with_context(|| format!("failed to resolve {}", path.display()))
}

fn load_corpus(args: &MeasureArgs) -> Result<Corpus> {
    ensure_default_clone(
        &args.registry_dir,
        DEFAULT_REGISTRY_DIR,
        "https://github.com/purescript/registry.git",
    )
    .context("failed to clone default registry")?;
    ensure_default_clone(
        &args.index_dir,
        DEFAULT_INDEX_DIR,
        "https://github.com/purescript/registry-index.git",
    )
    .context("failed to clone default registry index")?;

    let layout = RegistryLayout::new(&args.registry_dir, &args.index_dir);
    let registry = FsRegistry::new(layout);
    let package_set = registry
        .read_package_set(Some(&args.package_set))
        .with_context(|| format!("failed to read package set {}", args.package_set))?;
    let packages = package_map(&package_set);
    let selection = resolve_selection(&registry, &packages, &[], &[args.preset])
        .context("failed to resolve package selection")?;
    if !selection.errors.is_empty() {
        for issue in selection.errors {
            eprintln!("verifier[{}]: {}", issue.kind, issue.message);
        }
        bail!("failed to resolve package selection");
    }

    let cache_dir = args.cache_dir.clone().unwrap_or_else(default_cache_dir);
    let cache = PackageCache::new(cache_dir);
    prepare_packages(&cache, &selection.packages, "Preparing measured packages")?;

    let source_files = selection.packages.par_iter().map(|package| {
        let extracted = cache.source_path(&package.name, &package.version);
        discover_sources(&package.name, &package.version, &extracted)
    });
    let source_files = source_files.collect::<Result<Vec<_>, _>>()?;
    let mut source_files = source_files.into_iter().flatten().collect::<Vec<_>>();
    source_files.sort_by(|a, b| {
        (&a.package, &a.version, &a.relative_path).cmp(&(&b.package, &b.version, &b.relative_path))
    });

    let sources = source_files.iter().map(|file| {
        let content = fs::read_to_string(&file.path)
            .with_context(|| format!("failed to read {}", file.path.display()))?;
        Ok((file.path.to_string_lossy().into_owned(), content))
    });
    let sources = sources.collect::<Result<Vec<_>>>()?;

    let preset = args.preset.to_possible_value().expect("presets are not skipped");
    Ok(Corpus {
        preset: preset.get_name().to_string(),
        package_set: package_set.version.to_string(),
        sources,
    })
}

fn read_measurement(path: &Path) -> Result<Measurement> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_measurement(path: &Path, measurement: &Measurement) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(measurement)?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

pub fn compare(args: ComparePerformanceArgs) -> Result<bool> {
    let base = args.base.iter().map(|path| read_measurement(path));
    let base = base.collect::<Result<Vec<_>>>()?;
    let candidate = args.candidate.iter().map(|path| read_measurement(path));
    let candidate = candidate.collect::<Result<Vec<_>>>()?;

    let comparison = comparison::compare(
        &base,
        &candidate,
        args.threshold_percent,
        args.base_label,
        args.candidate_label,
    )?;

    if let Some(path) = &args.json_output {
        let content = serde_json::to_string_pretty(&comparison)?;
        fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    }

    let markdown = comparison::render_markdown(&comparison);
    match &args.summary_output {
        Some(path) => fs::write(path, &markdown)
            .with_context(|| format!("failed to write {}", path.display()))?,
        None => print!("{markdown}"),
    }

    Ok(comparison.has_regressions())
}
