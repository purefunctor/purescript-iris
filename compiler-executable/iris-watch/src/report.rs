//! Watch summaries and warnings printed to the terminal.

use std::path::Path;
use std::time::Duration;

use iris_build::{InputChange, InputChanges};
use iris_progress::{WatchOutcome, WatchSummary, render_watch_summary};
use itertools::Itertools;

use crate::WatchConfig;

pub(crate) fn operational_failure(
    config: &WatchConfig,
    root: &Path,
    inputs: &[InputChange],
    initial: bool,
    duration: Duration,
    error: &dyn std::error::Error,
) {
    tracing::error!(%error, "Watch compilation failed");
    eprintln!("Watch build failed: {error}");
    summary(config, root, inputs, initial, duration, WatchOutcome::Failed);
}

pub(crate) fn warnings(changes: &InputChanges) {
    for warning in &changes.warnings {
        tracing::warn!("{warning}");
        eprintln!("Watch warning: {warning}");
    }
}

pub(crate) fn summary(
    config: &WatchConfig,
    root: &Path,
    inputs: &[InputChange],
    initial: bool,
    duration: Duration,
    outcome: WatchOutcome,
) {
    if config.quiet {
        return;
    }
    let changed_inputs = inputs.iter().map(|input| input_label(input, root));
    let mut changed_inputs = changed_inputs.collect_vec();
    changed_inputs.sort();
    let timestamp = jiff::Zoned::now().strftime("%H:%M:%S").to_string();
    println!(
        "{}",
        render_watch_summary(
            WatchSummary {
                timestamp: &timestamp,
                initial,
                changed_inputs: &changed_inputs,
                duration,
                outcome,
            },
            config.color,
        )
    );
}

fn input_label(input: &InputChange, root: &Path) -> String {
    input.module_name.clone().unwrap_or_else(|| {
        input.source_path.strip_prefix(root).unwrap_or(&input.source_path).display().to_string()
    })
}
