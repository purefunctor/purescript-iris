//! Typed notifications emitted by build execution.

use std::time::Duration;

use iris_progress::{ProgressEvent, ProgressOutcome, ProgressReporter};

use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildOutcome {
    Succeeded,
    Diagnostics,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvent {
    Preparing,
    PlanReady { package_count: usize },
    PackageCompleted { package_name: String, duration: Duration },
    Finalizing { duration: Duration },
    Finished { duration: Duration, outcome: BuildOutcome },
}

pub trait BuildEventSink: Sync {
    /// A `Finished` event must not return while the sink can still write progress output.
    fn send(&self, event: BuildEvent);
}

pub struct ProgressEventSink {
    reporter: ProgressReporter,
}

impl ProgressEventSink {
    pub fn new(reporter: ProgressReporter) -> ProgressEventSink {
        ProgressEventSink { reporter }
    }
}

impl BuildEventSink for ProgressEventSink {
    fn send(&self, event: BuildEvent) {
        let event = match event {
            BuildEvent::Preparing => ProgressEvent::Preparing,
            BuildEvent::PlanReady { package_count } => ProgressEvent::PlanReady { package_count },
            BuildEvent::PackageCompleted { package_name, duration } => {
                ProgressEvent::PackageCompleted { package_name, duration }
            }
            BuildEvent::Finalizing { duration } => ProgressEvent::Finalizing { duration },
            BuildEvent::Finished { duration, outcome } => ProgressEvent::Finished {
                duration,
                outcome: match outcome {
                    BuildOutcome::Succeeded => ProgressOutcome::Succeeded,
                    BuildOutcome::Diagnostics => ProgressOutcome::Diagnostics,
                },
            },
        };
        self.reporter.report(event);
    }
}

#[derive(Default)]
pub struct RecordedBuildEvents {
    events: Mutex<Vec<BuildEvent>>,
}

impl RecordedBuildEvents {
    pub fn into_events(self) -> Vec<BuildEvent> {
        self.events
            .into_inner()
            .expect("invariant violated: recorded build events are not poisoned")
    }
}

impl BuildEventSink for RecordedBuildEvents {
    fn send(&self, event: BuildEvent) {
        self.events
            .lock()
            .expect("invariant violated: recorded build events are not poisoned")
            .push(event);
    }
}
