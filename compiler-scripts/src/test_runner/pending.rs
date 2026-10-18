use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use anyhow::{Context, anyhow, bail};
use console::style;
use insta::Snapshot;
use serde::Deserialize;

use crate::test_runner::category::TestCategory;
use crate::test_runner::cli::RunArgs;
use crate::test_runner::decision::DecisionInput;
use crate::test_runner::{decision, ui};

#[derive(Deserialize)]
struct PendingSnapshotJson {
    path: String,
}

pub struct PendingResult {
    pub count: usize,
    pub excluded_count: usize,
    pub total_lines_changed: usize,
}

pub struct SnapshotInfo {
    pub snapshot_path: String,
    pub short_path: String,
    pub snap_new: PathBuf,
    pub is_update: bool,
}

impl SnapshotInfo {
    /// Returns the short path with `.new` suffix for display (e.g., `foo.snap.new`)
    pub fn short_path_new(&self) -> String {
        format!("{}.new", self.short_path)
    }
}

/// Collect pending snapshots for a category, optionally filtered.
fn collect_pending_snapshots(
    category: TestCategory,
    filters: &[String],
) -> anyhow::Result<Vec<SnapshotInfo>> {
    let pending_output = Command::new("cargo")
        .arg("insta")
        .arg("pending-snapshots")
        .arg("--as-json")
        .output()
        .context("failed to run cargo insta")?;
    if !pending_output.status.success() {
        bail!(
            "cargo insta pending-snapshots failed with {}:\n{}",
            pending_output.status,
            String::from_utf8_lossy(&pending_output.stderr).trim_end()
        );
    }

    let pending = String::from_utf8_lossy(&pending_output.stdout);
    let pending = pending.trim();

    let cwd = env::current_dir().context("failed to get working directory")?;
    let path_fragments = category.snapshot_path_fragments();

    let mut snapshots = Vec::new();

    for line in pending.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let snapshot: PendingSnapshotJson = serde_json::from_str(line)
            .with_context(|| format!("unexpected cargo insta output: {line}"))?;
        let snapshot_path = snapshot.path;

        if !path_fragments.iter().any(|f| snapshot_path.contains(f)) {
            continue;
        }

        if !filters.is_empty() && !filters.iter().any(|f| snapshot_path.contains(f)) {
            continue;
        }

        let short_path = snapshot_path
            .strip_prefix(cwd.to_str().unwrap_or(""))
            .unwrap_or(&snapshot_path)
            .trim_start_matches('/')
            .to_string();

        let snap_new = PathBuf::from(format!("{}.new", snapshot_path));
        let is_update = Path::new(&snapshot_path).exists();

        snapshots.push(SnapshotInfo { snapshot_path, short_path, snap_new, is_update });
    }

    Ok(snapshots)
}

fn collect_exclusion_patterns(args: &RunArgs) -> Vec<String> {
    let mut patterns = args.exclude.clone();

    if let Ok(env_patterns) = env::var("EXCLUDE_SNAPSHOTS") {
        for pattern in env_patterns.split(',') {
            let pattern = pattern.trim();
            if !pattern.is_empty() {
                patterns.push(pattern.to_string());
            }
        }
    }

    patterns
}

fn apply_exclusions(
    snapshots: Vec<SnapshotInfo>,
    patterns: &[String],
) -> (Vec<SnapshotInfo>, usize) {
    if patterns.is_empty() {
        return (snapshots, 0);
    }

    let (excluded, visible): (Vec<_>, Vec<_>) = snapshots
        .into_iter()
        .partition(|info| patterns.iter().any(|p| info.short_path.contains(p)));

    (visible, excluded.len())
}

pub fn collect_selected_snapshots(
    category: TestCategory,
    args: &RunArgs,
) -> anyhow::Result<(Vec<SnapshotInfo>, usize)> {
    let snapshots = collect_pending_snapshots(category, &args.filters)?;

    // Apply exclusion filters
    let exclusion_patterns = collect_exclusion_patterns(args);
    let (visible, excluded_count) = apply_exclusions(snapshots, &exclusion_patterns);

    if excluded_count > 0 {
        println!(
            "{}",
            style(format!("info: excluded {} snapshot(s) by pattern", excluded_count)).dim()
        );
    }

    Ok((visible, excluded_count))
}

pub fn process_pending_snapshots(
    category: TestCategory,
    args: &RunArgs,
) -> anyhow::Result<PendingResult> {
    let (visible, excluded_count) = collect_selected_snapshots(category, args)?;

    let pending_count = visible.len();
    let mut total_lines_changed = 0;

    let limits = decision::decide_snapshot_limits(&DecisionInput {
        tests_passed: true, // not relevant for snapshot limits
        pending_count,
        total_lines_changed: 0, // not known yet, not relevant for limits
        showed_diffs: args.diff,
        ran_all: args.filters.is_empty(),
        max_count: args.count,
    });

    let max_shown = limits.max_shown;

    for info in visible.iter().take(max_shown) {
        let snap = Path::new(&info.snapshot_path);
        let stats = if info.is_update {
            ui::display_snapshot_diff(snap, &info.snap_new, &info.short_path_new(), args.diff)
        } else {
            ui::display_new_snapshot(&info.snap_new, &info.short_path_new(), args.diff)
        };
        total_lines_changed += stats.added + stats.removed;
    }

    if pending_count > max_shown {
        let hidden = pending_count - max_shown;
        println!(
            "{}",
            style(format!("...and {} more pending snapshot(s) not shown", hidden)).dim()
        );
    }

    Ok(PendingResult { count: visible.len(), excluded_count, total_lines_changed })
}

pub struct AcceptRejectResult {
    pub accepted: usize,
    pub rejected: usize,
    pub failed: usize,
}

pub fn accept_snapshots(snapshots: &[SnapshotInfo]) -> AcceptRejectResult {
    let mut accepted = 0;
    let mut failed = 0;

    for info in snapshots {
        if !info.snap_new.exists() {
            println!(
                "{} {} (missing .new file)",
                style("SKIP").yellow().bold(),
                style(&info.short_path).cyan()
            );
            failed += 1;
            continue;
        }

        if let Err(e) = persist_snapshot(&info.snap_new, Path::new(&info.snapshot_path)) {
            println!("{} {} ({:#})", style("FAIL").red().bold(), style(&info.short_path).cyan(), e);
            failed += 1;
            continue;
        }

        println!("{} {}", style("ACCEPTED").green().bold(), style(&info.short_path).cyan());
        accepted += 1;
    }

    AcceptRejectResult { accepted, rejected: 0, failed }
}

fn persist_snapshot(snap_new: &Path, snap_path: &Path) -> anyhow::Result<()> {
    let snapshot = Snapshot::from_file(snap_new).map_err(|error| anyhow!("{error}"))?;
    snapshot.save(snap_path).map_err(|error| anyhow!("{error}"))?;
    fs::remove_file(snap_new)
        .with_context(|| format!("failed to remove {}", snap_new.display()))?;
    Ok(())
}

pub fn reject_snapshots(snapshots: &[SnapshotInfo]) -> AcceptRejectResult {
    let mut rejected = 0;
    let mut failed = 0;

    for info in snapshots {
        if !info.snap_new.exists() {
            println!(
                "{} {} (missing .new file)",
                style("SKIP").yellow().bold(),
                style(&info.short_path).cyan()
            );
            failed += 1;
            continue;
        }

        if let Err(e) = fs::remove_file(&info.snap_new) {
            println!("{} {} ({})", style("FAIL").red().bold(), style(&info.short_path).cyan(), e);
            failed += 1;
            continue;
        }

        println!("{} {}", style("REJECTED").red().bold(), style(&info.short_path).cyan());
        rejected += 1;
    }

    AcceptRejectResult { accepted: 0, rejected, failed }
}
