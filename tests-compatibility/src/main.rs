mod performance;
mod verifier;

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use git2::FetchOptions;
use git2::build::RepoBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use tests_compatibility::registry::{FsRegistry, RegistryLayout, RegistryReader};
use tests_compatibility::{PackageCache, Preset, default_cache_dir};

use crate::verifier::cli::{Cli, Command, DEFAULT_INDEX_DIR, DEFAULT_REGISTRY_DIR};
use crate::verifier::comparison::compare_reports;
use crate::verifier::compile::compile_sources;
use crate::verifier::report::{PackageSetReport, Report, SelectionReport};
use crate::verifier::reporting::{append_markdown, github_annotations, render_markdown};
use crate::verifier::selection::{SelectedPackage, package_map, resolve_selection};
use crate::verifier::sources::discover_sources;

fn main() -> ExitCode {
    match run() {
        Ok(has_errors) => {
            if has_errors {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("verifier: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();

    match cli.command {
        Command::Prepare(args) => {
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
                .read_package_set(args.package_set.as_deref())
                .context("failed to read package set")?;
            let packages = package_map(&package_set);
            let presets = if args.presets.is_empty() {
                vec![Preset::Core, Preset::Acme]
            } else {
                args.presets
            };
            let selection = resolve_selection(&registry, &packages, &[], &presets)
                .context("failed to resolve package selection")?;

            if !selection.errors.is_empty() {
                for issue in selection.errors {
                    eprintln!("verifier[{}]: {}", issue.kind, issue.message);
                }
                anyhow::bail!("failed to resolve package selection");
            }

            let cache_dir = args.cache_dir.unwrap_or_else(default_cache_dir);
            let cache = PackageCache::new(cache_dir);
            prepare_packages(&cache, &selection.packages, "Preparing packages")?;

            println!("Prepared {} packages", selection.packages.len());
            Ok(false)
        }
        Command::Verify(args) => {
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
                .read_package_set(args.package_set.as_deref())
                .context("failed to read package set")?;
            let packages = package_map(&package_set);
            let selection = resolve_selection(&registry, &packages, &args.packages, &args.presets)
                .context("failed to resolve package selection")?;

            let cache_dir = args.cache_dir.unwrap_or_else(default_cache_dir);
            let cache = PackageCache::new(cache_dir);
            prepare_packages(&cache, &selection.packages, "Preparing selected packages")?;

            let source_files = selection.packages.par_iter().map(|package| {
                let extracted = cache.source_path(&package.name, &package.version);
                discover_sources(&package.name, &package.version, &extracted).with_context(|| {
                    format!(
                        "failed to discover source files for {}@{}",
                        package.name, package.version
                    )
                })
            });
            let source_files = source_files.collect::<Result<Vec<_>>>()?;
            let source_files = source_files.into_iter().flatten();
            let mut source_files = source_files.collect::<Vec<_>>();
            source_files.sort_by(|a, b| {
                (&a.package, &a.version, &a.relative_path).cmp(&(
                    &b.package,
                    &b.version,
                    &b.relative_path,
                ))
            });

            let mut report = Report::new(
                PackageSetReport {
                    version: package_set.version,
                    compiler: package_set.compiler,
                    published: package_set.published,
                },
                SelectionReport {
                    mode: selection.mode,
                    requested_packages: args.packages,
                    resolved_packages: selection.packages,
                },
            );
            report.verifier_errors = selection.errors;

            let compile_report = compile_sources(&source_files)
                .context("failed to compile selected source files")?;
            report.summary.source_files = source_files.len();
            report.diagnostics = compile_report.diagnostics;
            report.verifier_errors.extend(compile_report.verifier_errors);
            report.recompute_summary();

            report.print_human();

            if let Some(path) = args.json_output {
                report.write_json(&path).with_context(|| {
                    format!("failed to write JSON report to {}", path.display())
                })?;
            }

            Ok(report.has_errors())
        }
        Command::Compare(args) => {
            let base = Report::read_json(&args.base_report).with_context(|| {
                format!("failed to read base report from {}", args.base_report.display())
            })?;
            let candidate = Report::read_json(&args.candidate_report).with_context(|| {
                format!("failed to read candidate report from {}", args.candidate_report.display())
            })?;
            let comparison = compare_reports(&base, &candidate)?;
            comparison.write_json(&args.json_output).with_context(|| {
                format!("failed to write comparison report to {}", args.json_output.display())
            })?;

            let markdown = render_markdown(&comparison, &candidate, args.detail_limit);
            if let Some(path) = args.summary_output {
                append_markdown(&path, &markdown).with_context(|| {
                    format!("failed to append comparison summary to {}", path.display())
                })?;
            } else {
                println!("{markdown}");
            }
            if args.github_annotations {
                print!(
                    "{}",
                    github_annotations(
                        &comparison,
                        args.error_annotation_limit,
                        args.warning_annotation_limit,
                    )
                );
            }

            Ok(comparison.has_regressions())
        }
        Command::Measure(args) => {
            performance::measure(args)?;
            Ok(false)
        }
        Command::ComparePerformance(args) => performance::compare(args),
    }
}

pub(crate) fn prepare_packages(
    cache: &PackageCache,
    packages: &[SelectedPackage],
    message: &'static str,
) -> Result<()> {
    let pending = packages
        .iter()
        .filter(|package| !cache.is_package_prepared(&package.name, &package.version));
    let pending = pending.collect::<Vec<_>>();

    if pending.is_empty() {
        return Ok(());
    }

    let progress = package_progress(pending.len() as u64, message);
    pending.par_iter().try_for_each(|package| -> anyhow::Result<()> {
        progress.set_message(format!("{message}: {}@{}", package.name, package.version));
        cache.ensure_package(&package.name, &package.version).with_context(|| {
            format!("failed to prepare package {}@{}", package.name, package.version)
        })?;
        progress.inc(1);
        anyhow::Result::Ok(())
    })?;

    progress.finish_with_message(format!("Prepared {} packages", pending.len()));
    Ok(())
}

fn package_progress(total: u64, message: &'static str) -> ProgressBar {
    let progress = ProgressBar::new(total);
    let style = ProgressStyle::with_template(
        "{spinner:.green} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
    )
    .expect("progress bar template is valid")
    .progress_chars("##-");
    progress.set_style(style);
    progress.set_message(message);
    progress
}

pub(crate) fn ensure_default_clone(path: &Path, default_path: &str, url: &str) -> Result<()> {
    if path != Path::new(default_path) || path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }

    eprintln!("verifier: cloning {url} into {}", path.display());
    clone_shallow(url, path)?;

    Ok(())
}

fn clone_shallow(url: &str, path: &Path) -> Result<()> {
    let mut fetch_options = FetchOptions::new();
    fetch_options.depth(1);

    let mut builder = RepoBuilder::new();
    builder.fetch_options(fetch_options);
    builder
        .clone(url, path)
        .map(|_| ())
        .with_context(|| format!("failed to clone {url} into {}", path.display()))
}
