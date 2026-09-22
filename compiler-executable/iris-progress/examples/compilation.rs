use std::thread;
use std::time::Duration;

use iris_progress::{ProgressEvent, ProgressOutcome, ProgressRuntime};

fn main() {
    let packages = [
        ("prelude", 84),
        ("control", 112),
        ("effect", 97),
        ("either", 136),
        ("foldable-traversable", 164),
        ("maybe", 121),
        ("arrays", 188),
        ("strings", 143),
        ("console", 73),
        ("aff", 216),
        ("web-html", 179),
        ("application", 248),
    ];

    let runtime = ProgressRuntime::start(true, true);
    let reporter = runtime.reporter();
    if !reporter.is_active() {
        eprintln!("Run this example in an interactive terminal to display the progress bar.");
        return;
    }

    thread::sleep(Duration::from_millis(500));
    reporter.report(ProgressEvent::PlanReady { package_count: packages.len() });

    for (package_name, milliseconds) in packages {
        thread::sleep(Duration::from_millis(180));
        reporter.report(ProgressEvent::PackageCompleted {
            package_name: package_name.into(),
            duration: Duration::from_millis(milliseconds),
        });
    }

    let duration = Duration::from_millis(2_660);
    reporter.report(ProgressEvent::Finalizing { duration });
    thread::sleep(Duration::from_millis(500));
    reporter.report(ProgressEvent::Finished { duration, outcome: ProgressOutcome::Succeeded });
}
