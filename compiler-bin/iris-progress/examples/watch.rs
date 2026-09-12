use std::thread;
use std::time::Duration;

use iris_progress::{WatchOutcome, WatchSummary, render_watch_summary};
use itertools::Itertools;

fn main() {
    let examples = [
        (
            "14:32:08",
            true,
            vec!["Application", "Data.Route", "Effect.Fetch", "Main", "UI.Component"],
            Duration::from_millis(847),
            WatchOutcome::Succeeded,
        ),
        (
            "14:32:14",
            false,
            vec!["UI.Component"],
            Duration::from_millis(34),
            WatchOutcome::Succeeded,
        ),
        (
            "14:32:21",
            false,
            vec!["Application", "Main"],
            Duration::from_millis(91),
            WatchOutcome::Diagnostics,
        ),
    ];

    for (timestamp, initial, inputs, duration, outcome) in examples {
        let changed_inputs = inputs.into_iter().map(str::to_owned).collect_vec();
        println!(
            "{}",
            render_watch_summary(
                WatchSummary {
                    timestamp,
                    initial,
                    changed_inputs: &changed_inputs,
                    duration,
                    outcome
                },
                true,
            )
        );
        println!();
        thread::sleep(Duration::from_millis(350));
    }
}
