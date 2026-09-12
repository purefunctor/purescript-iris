use std::ffi::OsStr;
use std::path::PathBuf;

use itertools::Itertools;
use usage::{Args, Subcommands, ValueEnum};

use crate::build::project::BuildConfig;

#[derive(Debug, usage::Cli)]
#[usage(
    bin = "iris-v2",
    about = env!("CARGO_PKG_DESCRIPTION"),
    version = crate::VERSION,
    unknown_flags = "error",
    args_override_self = false
)]
pub struct Program {
    #[usage(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommands)]
pub enum Command {
    /// Build a Spago workspace or package.
    Build(BuildOptions),
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct BuildOptions {
    /// Workspace package to build.
    #[usage(short, long, value_name = "NAME")]
    package: Option<String>,

    /// Output directory. Defaults to output in the workspace root.
    #[usage(short, long, value_name = "DIR")]
    output: Option<PathBuf>,

    /// Suppress build progress and Spago output.
    #[usage(short, long)]
    quiet: bool,

    /// When to use colors in diagnostics and progress output.
    #[usage(long, value_enum, default = "auto")]
    color: ColorChoice,

    /// Write JavaScript output even when compilation reports errors.
    #[usage(long)]
    resilient: bool,

    /// Suppress compiler warnings and errors without hiding build progress.
    #[usage(long)]
    no_diagnostics: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl Command {
    pub fn into_config(self) -> BuildConfig {
        let Command::Build(options) = self;
        BuildConfig {
            package: options.package,
            output: options.output,
            quiet: options.quiet,
            color: options.color,
            resilient: options.resilient,
            diagnostics: !options.no_diagnostics,
        }
    }
}

impl Program {
    pub fn parse_with_diagnostics() -> Program {
        let arguments = std::env::args_os().collect_vec();
        let arguments = arguments.iter().map(|argument| argument.as_os_str()).collect_vec();
        match Program::try_parse_from(&arguments) {
            Ok(program) => program,
            Err(usage::Error::Help { cmd, long }) => {
                let page = usage::help::render_styled(
                    Program::spec(),
                    cmd,
                    long,
                    usage::help::Style::auto(),
                );
                print!("{}", page.unwrap_or_default());
                std::process::exit(0);
            }
            Err(usage::Error::HelpAll { cmd }) => {
                let page = usage::help::render_all_styled(
                    Program::spec(),
                    cmd,
                    usage::help::Style::auto(),
                );
                print!("{}", page.unwrap_or_default());
                std::process::exit(0);
            }
            Err(usage::Error::Version { .. }) => {
                println!("iris-v2 {}", crate::VERSION);
                std::process::exit(0);
            }
            Err(error) => {
                report_error(&arguments, error);
                std::process::exit(2);
            }
        }
    }
}

fn report_error(arguments: &[&OsStr], error: usage::Error) {
    let report = usage::diagnostic::report(Program::spec(), &arguments[1..], &error);
    eprint!("{}", report.rendered);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_only_the_build_command() {
        let arguments = ["iris-v2", "build", "--quiet", "--no-diagnostics", "--resilient"];
        let arguments = arguments.iter().map(OsStr::new).collect_vec();
        let program = Program::try_parse_from(&arguments).unwrap();
        let config = program.command.into_config();

        assert!(config.quiet);
        assert!(!config.diagnostics);
        assert!(config.resilient);
    }

    #[test]
    fn rejects_other_commands() {
        let arguments = [OsStr::new("iris-v2"), OsStr::new("lsp")];
        assert!(Program::try_parse_from(&arguments).is_err());
    }
}
