use std::env;
use std::ffi::OsStr;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use iris_build::{BuildConfig, ProjectConfig, RunConfig, TestConfig};
use iris_package::{AddConfig, NewConfig};
use iris_watch_server::protocol::{InstanceSearch, Namespace, Query};
use itertools::Itertools;
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
    /// Build a Spago workspace or package.
    Build(BuildOptions),
    /// Build a Spago workspace or package and rebuild when inputs change.
    Watch(WatchOptions),
    /// Run the language server over standard input and output.
    Lsp(LspOptions),
    /// Print agent skills for using Iris, which match this version.
    Skills(SkillsOptions),
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

    #[usage(subcommand)]
    command: Option<WatchCommand>,
}

#[derive(Debug, Subcommands)]
pub enum WatchCommand {
    /// Ask a running `iris watch` about the project.
    Query(QueryOptions),
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct QueryOptions {
    /// Output directory of the watcher. Defaults to output in the workspace root.
    #[usage(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Print the watcher's response as JSON.
    #[usage(long)]
    pub json: bool,

    #[usage(subcommand)]
    pub query: QueryCommand,
}

#[derive(Debug, Subcommands)]
pub enum QueryCommand {
    /// Rescan sources, rebuild if anything changed, and report the build outcome.
    Wait {},
    /// Print the signature of a value, or the kind of a type or class.
    Signature(ItemQuery),
    /// Print a module's exports with their signatures and documentation.
    Module(QueryName),
    /// Print where a value, type, or class is declared.
    Definition(ItemQuery),
    /// Print where a value, type, or class is used.
    References(ItemQuery),
    /// Print the modules that import a module, directly or through other modules.
    Dependents(QueryName),
    /// Print the instances of a class, or the instances whose head mentions a type.
    Instances(InstancesQuery),
    /// Print the declarations whose names match a pattern, best matches first.
    Search(SearchQuery),
    /// Print the diagnostics of one module, or of every module.
    Diagnostics(OptionalQueryName),
    /// Print the JavaScript generated for a module.
    Javascript(QueryName),
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct QueryName {
    /// Qualified name, such as Data.Maybe.fromMaybe or Data.Maybe.
    #[usage(value_name = "NAME")]
    name: String,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct SearchQuery {
    /// Part of a name, such as foldl or fromMay; letters may be skipped.
    #[usage(value_name = "PATTERN")]
    pattern: String,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct InstancesQuery {
    #[usage(subcommand)]
    search: InstancesCommand,
}

#[derive(Debug, Subcommands)]
pub enum InstancesCommand {
    /// The instances of a class.
    Class(QueryName),
    /// The instances whose head mentions a type, whatever their class.
    Type(QueryName),
}

/// A query about an item, answered for both the value and the type or class with that name
/// unless a namespace subcommand selects one.
#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct ItemQuery {
    /// Qualified name, such as Data.Maybe.fromMaybe or Data.Maybe.Maybe.
    #[usage(value_name = "NAME")]
    name: Option<String>,

    #[usage(subcommand)]
    namespace: Option<NamespaceCommand>,
}

#[derive(Debug, Subcommands)]
pub enum NamespaceCommand {
    /// Only the value with this name, such as a function or a data constructor.
    Value(QueryName),
    /// Only the type or class with this name.
    Type(QueryName),
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct OptionalQueryName {
    /// Module name, such as Data.Maybe.
    #[usage(value_name = "MODULE")]
    name: Option<String>,
}

impl QueryCommand {
    /// The query the watcher receives, or why the arguments do not form one.
    pub fn into_query(self) -> Result<Query, String> {
        let query = match self {
            QueryCommand::Wait {} => Query::Wait,
            QueryCommand::Signature(item) => {
                let (name, namespace) = item.into_parts()?;
                Query::Signature { name, namespace }
            }
            QueryCommand::Module(QueryName { name }) => Query::Module { name },
            QueryCommand::Definition(item) => {
                let (name, namespace) = item.into_parts()?;
                Query::Definition { name, namespace }
            }
            QueryCommand::References(item) => {
                let (name, namespace) = item.into_parts()?;
                Query::References { name, namespace }
            }
            QueryCommand::Dependents(QueryName { name }) => Query::Dependents { name },
            QueryCommand::Instances(InstancesQuery { search }) => match search {
                InstancesCommand::Class(QueryName { name }) => {
                    Query::Instances { name, search: InstanceSearch::Class }
                }
                InstancesCommand::Type(QueryName { name }) => {
                    Query::Instances { name, search: InstanceSearch::Type }
                }
            },
            QueryCommand::Search(SearchQuery { pattern }) => Query::Search { pattern },
            QueryCommand::Diagnostics(OptionalQueryName { name }) => Query::Diagnostics { name },
            QueryCommand::Javascript(QueryName { name }) => Query::Javascript { name },
        };
        Ok(query)
    }
}

impl ItemQuery {
    fn into_parts(self) -> Result<(String, Option<Namespace>), String> {
        match (self.name, self.namespace) {
            (Some(name), None) => Ok((name, None)),
            (None, Some(NamespaceCommand::Value(QueryName { name }))) => {
                Ok((name, Some(Namespace::Value)))
            }
            (None, Some(NamespaceCommand::Type(QueryName { name }))) => {
                Ok((name, Some(Namespace::Type)))
            }
            (None, None) => Err("expected a qualified name, or `value` or `type` and one".into()),
            (Some(name), Some(_)) => {
                Err(format!("`{name}` must come after the namespace, not before it"))
            }
        }
    }
}

impl WatchOptions {
    /// Takes `iris watch query`'s options. A query without its own `--output` reads the one given
    /// to `iris watch`, so a watcher's command line still finds it with `query` appended.
    pub fn take_query(&mut self) -> Option<QueryOptions> {
        let Some(WatchCommand::Query(mut query)) = self.command.take() else { return None };
        query.output = query.output.or_else(|| self.output.take());
        Some(query)
    }

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
                version: crate::VERSION.to_string(),
            },
        }
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct SkillsOptions {
    #[usage(subcommand)]
    pub command: Option<SkillsCommand>,
}

#[derive(Debug, Subcommands)]
pub enum SkillsCommand {
    /// List the skills with their descriptions. This is the default.
    List {},
    /// Print a skill.
    Get(SkillName),
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct SkillName {
    /// Skill name, as listed by `iris skills list`.
    #[usage(value_name = "NAME")]
    pub name: String,
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
}

impl LspOptions {
    pub fn into_config(self) -> LoggingFilters {
        LoggingFilters { query: self.query_log, checking: self.checking_log, lsp: self.lsp_log }
    }
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
