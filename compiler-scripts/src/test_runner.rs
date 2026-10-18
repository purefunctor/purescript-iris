mod category;
mod cli;
mod decision;
mod fixture;
mod nextest;
mod pending;
mod ui;

pub use category::TestCategory;
pub use cli::RunArgs;

use console::style;

use crate::test_runner::ui::NextActionsArgs;

pub fn run_category(category: TestCategory, args: &RunArgs) -> anyhow::Result<bool> {
    if matches!(category, TestCategory::Compiler | TestCategory::SourceMap) {
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        tests_support::prepare(
            repository.join("tests-integration/packages.json"),
            repository.join("target/integration-packages"),
        )?;
    }
    let tests_passed = nextest::run_nextest(category, args)?;
    let pending_result = pending::process_pending_snapshots(category, args)?;
    ui::print_next_actions(NextActionsArgs {
        category_name: category.as_str(),
        filters: &args.filters,
        exclusions: &args.exclude,
        tests_passed,
        pending_count: pending_result.count,
        excluded_count: pending_result.excluded_count,
        total_lines_changed: pending_result.total_lines_changed,
        showed_diffs: args.diff,
    });

    Ok(tests_passed)
}

pub fn accept_category(category: TestCategory, args: &RunArgs) -> anyhow::Result<bool> {
    let (snapshots, _) = pending::collect_selected_snapshots(category, args)?;

    if snapshots.is_empty() {
        println!("{}", style("No pending snapshots found.").dim());
        return Ok(true);
    }

    if !args.confirm && args.filters.is_empty() {
        println!("{} pending snapshot(s) in {}", snapshots.len(), style(category.as_str()).cyan());
        println!();
        for info in &snapshots {
            println!("  {}", style(&info.short_path).dim());
        }
        println!();
        let exclusions = ui::exclusion_arguments(&args.exclude);
        println!(
            "To accept all, run: {}",
            style(format!("just t {}{} --accept --confirm", category.as_str(), exclusions)).cyan()
        );
        return Ok(false);
    }

    let result = pending::accept_snapshots(&snapshots);
    println!();
    println!("{}", style(format!("Accepted {} snapshot(s)", result.accepted)).green());

    Ok(result.failed == 0)
}

pub fn reject_category(category: TestCategory, args: &RunArgs) -> anyhow::Result<bool> {
    let (snapshots, _) = pending::collect_selected_snapshots(category, args)?;

    if snapshots.is_empty() {
        println!("{}", style("No pending snapshots found.").dim());
        return Ok(true);
    }

    let result = pending::reject_snapshots(&snapshots);
    println!();
    println!("{}", style(format!("Rejected {} snapshot(s)", result.rejected)).red());

    Ok(result.failed == 0)
}

pub use fixture::DeleteFixtureOutcome;

pub fn create_fixture(category: TestCategory, name: &str) -> anyhow::Result<()> {
    fixture::create_fixture(category, name).map(|_| ())
}

pub fn delete_fixture(
    category: TestCategory,
    name: &str,
    confirm: bool,
) -> anyhow::Result<DeleteFixtureOutcome> {
    fixture::delete_fixture(category, name, confirm)
}
