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

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    fn args(filters: &[&str], verbose: bool) -> RunArgs {
        RunArgs {
            create: None,
            delete: None,
            confirm: false,
            accept: false,
            reject: false,
            update_output: false,
            filters: filters.iter().map(|filter| (*filter).to_string()).collect(),
            verbose,
            diff: false,
            count: 3,
            exclude: Vec::new(),
        }
    }

    fn command_arguments(command: &Command) -> Vec<&OsStr> {
        command.get_args().collect()
    }

    fn environment_setting<'a>(command: &'a Command, name: &str) -> Option<Option<&'a OsStr>> {
        command.get_envs().find(|(variable, _)| *variable == name).map(|(_, value)| value)
    }

    #[test]
    fn compiler_runs_unified_reporter_with_filters() {
        let command = build_nextest_command(TestCategory::Compiler, &args(&["constructor"], false));
        let arguments = command_arguments(&command);

        assert_eq!(
            arguments,
            [
                "nextest",
                "run",
                "-p",
                "tests-integration",
                "--test",
                "compiler",
                "constructor",
                "--status-level=none",
            ]
        );
    }

    #[test]
    fn other_categories_keep_their_single_reporter_and_verbose_options() {
        let command = build_nextest_command(TestCategory::Lowering, &args(&[], true));
        let arguments = command_arguments(&command);

        assert_eq!(
            &arguments[..6],
            ["nextest", "run", "-p", "tests-integration", "--test", "lowering"]
        );
        assert!(arguments.contains(&OsStr::new("--status-level=fail")));
    }

    #[test]
    fn compiler_output_updates_are_explicitly_enabled_for_nextest() {
        let mut run_args = args(&["javascript_execution"], false);
        run_args.update_output = true;

        let command = build_nextest_command(TestCategory::Compiler, &run_args);

        assert_eq!(
            environment_setting(&command, UPDATE_JAVASCRIPT_OUTPUT),
            Some(Some(OsStr::new("1")))
        );
    }

    #[test]
    fn ordinary_test_runs_do_not_update_compiler_output() {
        let command = build_nextest_command(TestCategory::Compiler, &args(&[], false));

        assert_eq!(environment_setting(&command, UPDATE_JAVASCRIPT_OUTPUT), Some(None));
    }
}
