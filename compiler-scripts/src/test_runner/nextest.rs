use std::io::{self, IsTerminal};
use std::process::{Command, Stdio};

use anyhow::Context;
use console::style;

use crate::test_runner::category::TestCategory;
use crate::test_runner::cli::RunArgs;

const UPDATE_JAVASCRIPT_OUTPUT: &str = "IRIS_UPDATE_JAVASCRIPT_OUTPUT";

pub fn build_nextest_command(category: TestCategory, args: &RunArgs) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("nextest").arg("run").arg("-p").arg("tests-integration");

    for target in category.test_targets() {
        cmd.arg("--test").arg(target);
    }

    for filter in &args.filters {
        cmd.arg(filter);
    }

    if args.verbose {
        cmd.arg("--status-level=fail");
        if !io::stderr().is_terminal() {
            cmd.arg("--color=never");
        }
    } else {
        cmd.arg("--status-level=none");
    }

    cmd.env("INSTA_FORCE_PASS", "1");
    cmd.env("INSTA_UPDATE", "new");
    cmd.env_remove("INSTA_FORCE_UPDATE");
    cmd.env_remove("INSTA_FORCE_UPDATE_SNAPSHOTS");
    cmd.env_remove(UPDATE_JAVASCRIPT_OUTPUT);
    if matches!(category, TestCategory::Compiler) && args.update_output {
        cmd.env(UPDATE_JAVASCRIPT_OUTPUT, "1");
    }

    cmd
}

pub fn run_nextest(category: TestCategory, args: &RunArgs) -> anyhow::Result<bool> {
    let mut cmd = build_nextest_command(category, args);

    if args.verbose {
        let status = cmd.status().context("failed to run cargo nextest")?;
        Ok(status.success())
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let status = cmd.status().context("failed to run cargo nextest")?;

        if !status.success() {
            eprintln!("{}", style("Tests failed, re-running verbose...").yellow());

            let verbose_args = RunArgs {
                create: None,
                delete: None,
                confirm: false,
                accept: false,
                reject: false,
                update_output: args.update_output,
                filters: args.filters.clone(),
                verbose: true,
                diff: args.diff,
                count: args.count,
                exclude: args.exclude.clone(),
            };
            let mut retry = build_nextest_command(category, &verbose_args);
            retry.status().context("failed to re-run cargo nextest in verbose mode")?;
        }

        Ok(status.success())
    }
}
