use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use serde::Serialize;

use super::measurement::{InternedCounts, Measurement, Method, is_edit_stage};

pub const BUILD_TOTAL: &str = "build";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceComparison {
    pub threshold_percent: f64,
    pub base_label: String,
    pub candidate_label: String,
    pub counted: Vec<CountedComparison>,
    pub timed: Vec<TimedComparison>,
    pub regressions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountedComparison {
    pub preset: String,
    pub package_set: String,
    pub source_files: usize,
    pub stages: Vec<CountedStage>,
    pub base_interned: Option<InternedCounts>,
    pub candidate_interned: InternedCounts,
    pub base_output_digest: Option<String>,
    pub candidate_output_digest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountedStage {
    pub stage: String,
    pub base: Option<Counts>,
    pub candidate: Counts,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub instructions_outside_allocator: u64,
    pub allocator_instructions: u64,
    pub allocations: u64,
    pub allocated_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimedComparison {
    pub preset: String,
    pub threads: usize,
    pub pairs: usize,
    pub stages: Vec<TimedStage>,
    pub base_peak_resident_bytes: Option<u64>,
    pub candidate_peak_resident_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimedStage {
    pub stage: String,
    pub base_median_nanoseconds: Option<u64>,
    pub candidate_median_nanoseconds: u64,
    pub median_paired_ratio: Option<f64>,
}

impl PerformanceComparison {
    pub fn has_regressions(&self) -> bool {
        !self.regressions.is_empty()
    }
}

pub fn compare(
    base: &[Measurement],
    candidate: &[Measurement],
    threshold_percent: f64,
    base_label: String,
    candidate_label: String,
) -> Result<PerformanceComparison> {
    let base = group(base);
    let candidate = group(candidate);

    for (key, base_runs) in &base {
        if !candidate.contains_key(key) {
            bail!("the base has {} measurements that the candidate lacks", describe(key));
        }
        let candidate_runs = &candidate[key];
        check_comparable(&base_runs[0], &candidate_runs[0])?;
    }

    let mut comparison = PerformanceComparison {
        threshold_percent,
        base_label,
        candidate_label,
        counted: Vec::new(),
        timed: Vec::new(),
        regressions: Vec::new(),
    };

    for (key, candidate_runs) in &candidate {
        let base_runs = base.get(key);
        match key.1 {
            Method::Callgrind => {
                let counted = compare_counted(base_runs.map(|runs| &runs[0]), &candidate_runs[0]);
                comparison.regressions.extend(counted_regressions(&counted, threshold_percent));
                comparison.counted.push(counted);
            }
            Method::Timing => {
                comparison.timed.push(compare_timed(base_runs.map(Vec::as_slice), candidate_runs));
            }
        }
    }

    Ok(comparison)
}

type GroupKey = (String, Method, usize);

fn group(measurements: &[Measurement]) -> BTreeMap<GroupKey, Vec<Measurement>> {
    let mut groups: BTreeMap<GroupKey, Vec<Measurement>> = BTreeMap::new();
    for measurement in measurements {
        let key = (measurement.preset.clone(), measurement.method, measurement.threads);
        groups.entry(key).or_default().push(measurement.clone());
    }
    groups
}

fn describe((preset, method, threads): &GroupKey) -> String {
    format!("{preset} {method:?} ({threads} threads)")
}

fn check_comparable(base: &Measurement, candidate: &Measurement) -> Result<()> {
    if base.package_set != candidate.package_set {
        bail!(
            "the base measured package set {} and the candidate {}; pass the same --package-set to both",
            base.package_set,
            candidate.package_set
        );
    }
    if (base.source_files, base.source_bytes) != (candidate.source_files, candidate.source_bytes) {
        bail!(
            "the base measured {} files ({} bytes) and the candidate {} files ({} bytes) of {}",
            base.source_files,
            base.source_bytes,
            candidate.source_files,
            candidate.source_bytes,
            base.preset
        );
    }
    let base_stages = base.stages.iter().map(|stage| &stage.stage);
    let candidate_stages = candidate.stages.iter().map(|stage| &stage.stage);
    if !base_stages.eq(candidate_stages) {
        bail!("the base and the candidate measure different stages of {}", base.preset);
    }
    Ok(())
}

fn counts_of(measurement: &Measurement) -> Vec<(String, Counts)> {
    let mut build = Counts::default();
    let mut rows = Vec::new();
    for stage in &measurement.stages {
        let allocations = stage.allocations.unwrap_or_default();
        let allocator_instructions = stage.allocator_instructions.unwrap_or_default();
        let counts = Counts {
            instructions_outside_allocator: stage.instructions.unwrap_or_default()
                - allocator_instructions,
            allocator_instructions,
            allocations: allocations.allocations + allocations.reallocations,
            allocated_bytes: allocations.allocated_bytes,
        };
        if is_edit_stage(&stage.stage) {
            if !rows.iter().any(|(name, _)| name == BUILD_TOTAL) {
                rows.push((BUILD_TOTAL.to_string(), build));
            }
        } else {
            build.instructions_outside_allocator += counts.instructions_outside_allocator;
            build.allocator_instructions += counts.allocator_instructions;
            build.allocations += counts.allocations;
            build.allocated_bytes += counts.allocated_bytes;
        }
        rows.push((stage.stage.clone(), counts));
    }
    rows
}

fn compare_counted(base: Option<&Measurement>, candidate: &Measurement) -> CountedComparison {
    let base_counts = base.map(counts_of);
    let stages = counts_of(candidate).into_iter().enumerate().map(|(index, (stage, counts))| {
        let base = base_counts.as_ref().map(|rows| rows[index].1);
        CountedStage { stage, base, candidate: counts }
    });
    CountedComparison {
        preset: candidate.preset.clone(),
        package_set: candidate.package_set.clone(),
        source_files: candidate.source_files,
        stages: stages.collect(),
        base_interned: base.map(|base| base.interned),
        candidate_interned: candidate.interned,
        base_output_digest: base.map(|base| base.output_digest.clone()),
        candidate_output_digest: candidate.output_digest.clone(),
    }
}

pub fn change_percent(base: u64, candidate: u64) -> f64 {
    if base == candidate {
        0.0
    } else if base == 0 {
        f64::INFINITY
    } else {
        (candidate as f64 - base as f64) / base as f64 * 100.0
    }
}

fn counted_regressions(counted: &CountedComparison, threshold_percent: f64) -> Vec<String> {
    let mut regressions = Vec::new();
    let preset = &counted.preset;

    for stage in &counted.stages {
        let Some(base) = stage.base else { continue };
        let metrics = [
            (
                "instructions",
                base.instructions_outside_allocator,
                stage.candidate.instructions_outside_allocator,
            ),
            ("allocations", base.allocations, stage.candidate.allocations),
            ("allocated bytes", base.allocated_bytes, stage.candidate.allocated_bytes),
        ];
        for (metric, base, candidate) in metrics {
            let change = change_percent(base, candidate);
            if change > threshold_percent {
                regressions.push(format!(
                    "`{preset}` {}: {metric} {}",
                    stage.stage,
                    format_change(change)
                ));
            }
        }
    }

    if let Some(base) = counted.base_interned {
        let candidate = counted.candidate_interned;
        let metrics = [
            ("interned types", base.types, candidate.types),
            (
                "interned types with unification variables",
                base.types_with_unification,
                candidate.types_with_unification,
            ),
            ("interned row types", base.row_types, candidate.row_types),
        ];
        for (metric, base, candidate) in metrics {
            let change = change_percent(base as u64, candidate as u64);
            if change > threshold_percent {
                regressions.push(format!("`{preset}`: {metric} {}", format_change(change)));
            }
        }
    }

    if let Some(base_digest) = &counted.base_output_digest
        && *base_digest != counted.candidate_output_digest
    {
        regressions.push(format!("`{preset}`: generated JavaScript or source maps changed"));
    }

    regressions
}

fn compare_timed(base: Option<&[Measurement]>, candidate: &[Measurement]) -> TimedComparison {
    let pairs = base.map_or(0, |base| base.len().min(candidate.len()));
    let wall = |measurement: &Measurement, stage: &str| {
        if stage == BUILD_TOTAL {
            let build = measurement.stages.iter().filter(|stage| !is_edit_stage(&stage.stage));
            build.map(|stage| stage.wall_nanoseconds.unwrap_or_default()).sum()
        } else {
            let stage = measurement.stages.iter().find(|measured| measured.stage == stage);
            stage.and_then(|stage| stage.wall_nanoseconds).unwrap_or_default()
        }
    };

    let first = &candidate[0];
    let mut names = Vec::new();
    for stage in &first.stages {
        if is_edit_stage(&stage.stage) && !names.iter().any(|name| name == BUILD_TOTAL) {
            names.push(BUILD_TOTAL.to_string());
        }
        names.push(stage.stage.clone());
    }
    let stages = names.into_iter().map(|stage| {
        let candidate_walls = candidate.iter().map(|run| wall(run, &stage)).collect();
        let base_walls = base.map(|base| base.iter().map(|run| wall(run, &stage)).collect());
        let ratios = base.map(|base| {
            let pairs = base.iter().zip(candidate).take(pairs);
            let ratios = pairs.map(|(base, candidate)| {
                wall(candidate, &stage) as f64 / wall(base, &stage).max(1) as f64
            });
            ratios.collect::<Vec<_>>()
        });
        TimedStage {
            base_median_nanoseconds: base_walls.map(median_u64),
            candidate_median_nanoseconds: median_u64(candidate_walls),
            median_paired_ratio: ratios.filter(|ratios| !ratios.is_empty()).map(median_f64),
            stage,
        }
    });

    let peak = |runs: &[Measurement]| {
        let peaks = runs.iter().filter_map(|run| run.peak_resident_bytes).collect::<Vec<_>>();
        if peaks.is_empty() { None } else { Some(median_u64(peaks)) }
    };

    TimedComparison {
        preset: first.preset.clone(),
        threads: first.threads,
        pairs,
        stages: stages.collect(),
        base_peak_resident_bytes: base.and_then(peak),
        candidate_peak_resident_bytes: peak(candidate),
    }
}

fn median_u64(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn median_f64(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

pub fn format_change(change: f64) -> String {
    if change.is_infinite() { "new".to_string() } else { format!("{change:+.3}%") }
}

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(digit);
    }
    output
}

fn milliseconds(nanoseconds: u64) -> String {
    format!("{:.1} ms", nanoseconds as f64 / 1e6)
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

fn counted_cell(base: Option<u64>, candidate: u64) -> String {
    match base {
        Some(base) => {
            format!("{} ({})", grouped(candidate), format_change(change_percent(base, candidate)))
        }
        None => grouped(candidate),
    }
}

pub fn render_markdown(comparison: &PerformanceComparison) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "## Performance comparison\n");
    let _ = writeln!(
        output,
        "Base: `{}`. Candidate: `{}`.\n",
        comparison.base_label, comparison.candidate_label
    );

    if comparison.regressions.is_empty() {
        let _ = writeln!(
            output,
            "No gated metric regressed by more than {}%.\n",
            comparison.threshold_percent
        );
    } else {
        let _ = writeln!(output, "**Regressions above {}%:**\n", comparison.threshold_percent);
        for regression in &comparison.regressions {
            let _ = writeln!(output, "- {regression}");
        }
        output.push('\n');
    }

    for counted in &comparison.counted {
        let _ = writeln!(
            output,
            "### `{}`: instructions and allocations\n\nCallgrind, one thread, package set {}, {} files. \
             Each cell shows the candidate and its change from the base. Instructions exclude the \
             allocator, whose cost depends on the heap left by earlier stages; allocator instructions \
             are reported but not gated, and allocation counts gate allocator work.\n",
            counted.preset, counted.package_set, counted.source_files
        );
        let _ = writeln!(
            output,
            "| Stage | Instructions | Allocator instructions | Allocations | Allocated bytes |"
        );
        let _ = writeln!(output, "| --- | ---: | ---: | ---: | ---: |");
        for stage in &counted.stages {
            let label = if stage.stage == BUILD_TOTAL {
                format!("**{}**", stage.stage)
            } else {
                stage.stage.clone()
            };
            let base = stage.base;
            let _ = writeln!(
                output,
                "| {label} | {} | {} | {} | {} |",
                counted_cell(
                    base.map(|base| base.instructions_outside_allocator),
                    stage.candidate.instructions_outside_allocator
                ),
                counted_cell(
                    base.map(|base| base.allocator_instructions),
                    stage.candidate.allocator_instructions
                ),
                counted_cell(base.map(|base| base.allocations), stage.candidate.allocations),
                counted_cell(
                    base.map(|base| base.allocated_bytes),
                    stage.candidate.allocated_bytes
                ),
            );
        }
        output.push('\n');

        let base = counted.base_interned;
        let candidate = counted.candidate_interned;
        let _ = writeln!(output, "| Interned after the build | Count |");
        let _ = writeln!(output, "| --- | ---: |");
        let rows = [
            ("types", base.map(|base| base.types), candidate.types),
            (
                "types with unification variables",
                base.map(|base| base.types_with_unification),
                candidate.types_with_unification,
            ),
            ("forall binders", base.map(|base| base.forall_binders), candidate.forall_binders),
            ("row types", base.map(|base| base.row_types), candidate.row_types),
            ("strings", base.map(|base| base.smol_strs), candidate.smol_strs),
            ("string literals", base.map(|base| base.string_literals), candidate.string_literals),
        ];
        for (label, base, candidate) in rows {
            let cell = counted_cell(base.map(|base| base as u64), candidate as u64);
            let _ = writeln!(output, "| {label} | {cell} |");
        }
        output.push('\n');

        match &counted.base_output_digest {
            Some(base) if *base == counted.candidate_output_digest => {
                let _ = writeln!(
                    output,
                    "Generated JavaScript and source maps are identical (digest `{base}`).\n"
                );
            }
            Some(base) => {
                let _ = writeln!(
                    output,
                    "**Generated JavaScript or source maps changed** (base `{base}`, candidate `{}`).\n",
                    counted.candidate_output_digest
                );
            }
            None => {
                let _ = writeln!(output, "Output digest: `{}`.\n", counted.candidate_output_digest);
            }
        }
    }

    for timed in &comparison.timed {
        let _ = writeln!(
            output,
            "### `{}`: wall time, {} thread(s)\n\nReported, not gated. Medians over {} interleaved \
             base and candidate runs; the ratio is the median of candidate over base per pair.\n",
            timed.preset, timed.threads, timed.pairs
        );
        let _ = writeln!(output, "| Stage | Base | Candidate | Ratio |");
        let _ = writeln!(output, "| --- | ---: | ---: | ---: |");
        for stage in &timed.stages {
            let label = if stage.stage == BUILD_TOTAL {
                format!("**{}**", stage.stage)
            } else {
                stage.stage.clone()
            };
            let _ = writeln!(
                output,
                "| {label} | {} | {} | {} |",
                stage.base_median_nanoseconds.map_or("—".to_string(), milliseconds),
                milliseconds(stage.candidate_median_nanoseconds),
                stage.median_paired_ratio.map_or("—".to_string(), |ratio| format!("{ratio:.3}")),
            );
        }
        let _ = writeln!(
            output,
            "| peak resident memory | {} | {} | |\n",
            timed.base_peak_resident_bytes.map_or("—".to_string(), megabytes),
            timed.candidate_peak_resident_bytes.map_or("—".to_string(), megabytes),
        );
    }

    output
}

#[cfg(test)]
mod tests {
    use tests_compatibility::allocation::AllocationCounts;

    use super::compare;
    use crate::performance::measurement::{InternedCounts, Measurement, Method, StageMeasurement};

    fn measurement(checked_instructions: u64, output_digest: &str) -> Measurement {
        measurement_with_allocator(checked_instructions, 0, output_digest)
    }

    fn measurement_with_allocator(
        checked_instructions: u64,
        checked_allocator_instructions: u64,
        output_digest: &str,
    ) -> Measurement {
        let stage = |stage: &str, instructions: u64| StageMeasurement {
            stage: stage.to_string(),
            instructions: Some(instructions),
            allocator_instructions: Some(if stage == "checked" {
                checked_allocator_instructions
            } else {
                0
            }),
            allocations: Some(AllocationCounts {
                allocations: 100,
                reallocations: 10,
                allocated_bytes: 4096,
            }),
            wall_nanoseconds: None,
            resident_growth_bytes: None,
        };
        Measurement {
            preset: "core".to_string(),
            package_set: "82.1.1".to_string(),
            source_files: 2,
            source_bytes: 64,
            method: Method::Callgrind,
            threads: 1,
            stages: vec![
                stage("parsed", 1_000),
                stage("checked", checked_instructions),
                stage("edit_comment", 300),
                stage("edit_body", 400),
            ],
            interned: InternedCounts {
                types: 10,
                types_with_unification: 4,
                forall_binders: 1,
                row_types: 1,
                smol_strs: 1,
                string_literals: 1,
            },
            peak_resident_bytes: None,
            output_digest: output_digest.to_string(),
        }
    }

    fn regressions(base: Measurement, candidate: Measurement) -> Vec<String> {
        let comparison =
            compare(&[base], &[candidate], 0.5, "base".to_string(), "head".to_string()).unwrap();
        comparison.regressions
    }

    #[test]
    fn instruction_regressions_are_flagged_above_the_threshold() {
        let at_threshold = regressions(measurement(10_000, "a"), measurement(10_050, "a"));
        assert!(at_threshold.is_empty(), "{at_threshold:?}");

        let above_threshold = regressions(measurement(10_000, "a"), measurement(10_051, "a"));
        assert_eq!(above_threshold, ["`core` checked: instructions +0.510%"]);

        let improvement = regressions(measurement(10_000, "a"), measurement(5_000, "a"));
        assert!(improvement.is_empty(), "{improvement:?}");
    }

    #[test]
    fn allocator_instructions_are_not_gated() {
        let allocator_only = regressions(
            measurement_with_allocator(10_000, 1_000, "a"),
            measurement_with_allocator(10_500, 1_500, "a"),
        );
        assert!(allocator_only.is_empty(), "{allocator_only:?}");

        let outside_allocator = regressions(
            measurement_with_allocator(10_000, 1_000, "a"),
            measurement_with_allocator(10_500, 1_000, "a"),
        );
        assert_eq!(
            outside_allocator,
            ["`core` checked: instructions +5.556%", "`core` build: instructions +5.000%"]
        );
    }

    #[test]
    fn changed_output_is_a_regression_even_when_counts_improve() {
        let changed = regressions(measurement(10_000, "a"), measurement(9_000, "b"));
        assert_eq!(changed, ["`core`: generated JavaScript or source maps changed"]);
    }
}
