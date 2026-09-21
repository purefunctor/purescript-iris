use std::ffi::OsStr;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::{env, fs};

use configuration::{Configuration, ConfigurationSettings};
use iris_build::{BuildConfig, ProjectConfig, RunConfig, TestConfig};
use iris_package::{AddConfig, NewConfig};
use itertools::Itertools;
use thiserror::Error;
use tracing::level_filters::LevelFilter;
use usage::{Args, Subcommands, ValueEnum};

use crate::logging::LoggingFilters;

#[derive(Debug, usage::Cli)]
#[usage(
    bin = "iris",
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
    /// Create a Spago project in the current directory.
    New(NewOptions),
    /// Add dependencies to a Spago package.
    Add(AddOptions),
    /// Print skills that teach coding agents how to use Iris features.
    AgentSkills(AgentSkillsOptions),
    /// Build a Spago workspace or package.
    Build(BuildOptions),
    /// Build a Spago workspace or package and rebuild when inputs change.
    Watch(WatchOptions),
    /// Run the language server over standard input and output.
    Lsp(LspOptions),
    /// Build and run a Spago package with Node.js.
    Run {
        #[usage(flatten)]
        options: RunOptions,
    },
    /// Build and test one or more Spago packages with Node.js.
    Test {
        #[usage(flatten)]
        options: TestOptions,
    },
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct AgentSkillsOptions {
    #[usage(subcommand)]
    pub skill: AgentSkill,
}

#[derive(Debug, Subcommands)]
pub enum AgentSkill {
    /// Print the skill for building with native Sync and Async effects.
    Effect,
}

pub struct LspConfig {
    pub configuration: Configuration,
    pub logging: LoggingFilters,
}

pub struct WatchConfig {
    pub project: ProjectConfig,
    pub watch: iris_watch::WatchConfig,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct NewOptions {
    /// Package name. Defaults to the current directory name.
    #[usage(long, value_name = "NAME")]
    name: Option<String>,
}

impl NewOptions {
    pub fn into_config(self) -> NewConfig {
        NewConfig { name: self.name }
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct AddOptions {
    /// Workspace package whose dependencies should change.
    #[usage(short, long, value_name = "NAME")]
    package: Option<String>,

    /// Add packages as test dependencies.
    #[usage(long)]
    test: bool,

    /// Packages to add.
    #[usage(value_name = "DEPENDENCY", required = true)]
    dependencies: Vec<String>,
}

impl AddOptions {
    pub fn into_config(self) -> AddConfig {
        AddConfig {
            package: self.package,
            dependencies: self.dependencies,
            test_dependencies: self.test,
        }
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct BuildOptions {
    #[usage(flatten)]
    project: ProjectBuildOptions,

    /// Write JavaScript output even when compilation reports errors.
    #[usage(long)]
    resilient: bool,

    /// Suppress compiler warnings and errors without hiding build progress.
    #[usage(long)]
    no_diagnostics: bool,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct ProjectBuildOptions {
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl BuildOptions {
    pub fn into_config(self) -> BuildConfig {
        BuildConfig {
            package: self.project.package,
            output: self.project.output,
            quiet: self.project.quiet,
            color: use_color(self.project.color),
            resilient: self.resilient,
            diagnostics: !self.no_diagnostics,
        }
    }
}

impl ProjectBuildOptions {
    fn into_project_config(self) -> (ProjectConfig, bool) {
        let project =
            ProjectConfig { package: self.package, output: self.output, quiet: self.quiet };
        (project, use_color(self.color))
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct RunOptions {
    #[usage(flatten)]
    build: ProjectBuildOptions,

    /// Module containing the program entry point.
    #[usage(long, value_name = "MODULE")]
    main: Option<String>,

    /// Arguments passed to the program.
    #[usage(double_dash = "required")]
    arguments: Vec<String>,
}

impl RunOptions {
    pub fn into_config(self) -> RunConfig {
        let (project, color) = self.build.into_project_config();
        RunConfig { project, color, main: self.main, arguments: self.arguments }
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct TestOptions {
    #[usage(flatten)]
    build: ProjectBuildOptions,

    /// Module containing the test entry point.
    #[usage(long, value_name = "MODULE")]
    main: Option<String>,

    /// Arguments passed to each test program.
    #[usage(double_dash = "required")]
    arguments: Vec<String>,
}

impl TestOptions {
    pub fn into_config(self) -> TestConfig {
        let (project, color) = self.build.into_project_config();
        TestConfig { project, color, main: self.main, arguments: self.arguments }
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct WatchOptions {
    /// Workspace package to build.
    #[usage(short, long, value_name = "NAME")]
    package: Option<String>,

    /// Output directory. Defaults to output in the workspace root.
    #[usage(short, long, value_name = "DIR")]
    output: Option<PathBuf>,

    /// Suppress watch summaries and Spago output.
    #[usage(short, long)]
    quiet: bool,

    /// When to use colors in diagnostics and watch output.
    #[usage(long, value_enum, default = "auto")]
    color: ColorChoice,

    /// Suppress compiler warnings and errors without hiding watch summaries.
    #[usage(long)]
    no_diagnostics: bool,
}

impl WatchOptions {
    pub fn into_config(self) -> WatchConfig {
        WatchConfig {
            project: ProjectConfig {
                package: self.package,
                output: self.output,
                quiet: self.quiet,
            },
            watch: iris_watch::WatchConfig {
                quiet: self.quiet,
                color: use_color(self.color),
                diagnostics: !self.no_diagnostics,
            },
        }
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct LspOptions {
    /// Use standard input and output for LSP transport.
    #[usage(long)]
    stdio: bool,

    /// Log level for the query engine.
    #[usage(
        long,
        value_name = "LEVEL",
        default = "off",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    query_log: LevelFilter,

    /// Log level for the type checker.
    #[usage(
        long,
        value_name = "LEVEL",
        default = "off",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    checking_log: LevelFilter,

    /// Log level for the language server.
    #[usage(
        long,
        value_name = "LEVEL",
        default = "info",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    lsp_log: LevelFilter,

    /// Language server configuration as a JSON object, read once at startup.
    #[usage(long, value_name = "JSON", conflicts = "--config-file")]
    config: Option<String>,

    /// Language server configuration file, relative to the working directory.
    #[usage(long, value_name = "PATH", conflicts = "--config")]
    config_file: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("failed to read configuration file {}: {error}", path.display())]
    ReadFile { path: PathBuf, error: io::Error },
    #[error("invalid configuration in {input}: {error}")]
    InvalidJson { input: String, error: serde_json::Error },
}

impl LspOptions {
    pub fn into_config(self) -> Result<LspConfig, ConfigurationError> {
        let configuration = read_configuration(self.config, self.config_file)?;
        let logging = LoggingFilters {
            query: self.query_log,
            checking: self.checking_log,
            lsp: self.lsp_log,
        };
        Ok(LspConfig { configuration, logging })
    }
}

fn read_configuration(
    config: Option<String>,
    config_file: Option<PathBuf>,
) -> Result<Configuration, ConfigurationError> {
    let (input, content) = if let Some(path) = config_file {
        let content = fs::read_to_string(&path)
            .map_err(|error| ConfigurationError::ReadFile { path: PathBuf::clone(&path), error })?;
        (path.display().to_string(), content)
    } else if let Some(content) = config {
        ("--config".to_string(), content)
    } else {
        return Ok(Configuration::default());
    };
    let settings = serde_json::from_str::<Option<ConfigurationSettings>>(&content)
        .map_err(|error| ConfigurationError::InvalidJson { input, error })?
        .unwrap_or_default();
    Ok(settings.apply_to(&Configuration::default()))
}

fn use_color(choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Auto => {
            let no_color = env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
            io::stderr().is_terminal() && !no_color
        }
        ColorChoice::Always => true,
        ColorChoice::Never => false,
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
                println!("iris {}", crate::VERSION);
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
    fn maps_build_options_to_configuration() {
        let arguments = ["iris", "build", "--quiet", "--no-diagnostics", "--resilient"];
        let arguments = arguments.iter().map(OsStr::new).collect_vec();
        let program = Program::try_parse_from(&arguments).unwrap();
        let Command::Build(options) = program.command else {
            panic!("invariant violated: expected build command");
        };
        let config = options.into_config();

        assert!(config.quiet);
        assert!(!config.diagnostics);
        assert!(config.resilient);
    }
}
