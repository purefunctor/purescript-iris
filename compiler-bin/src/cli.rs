use std::borrow::Cow;
use std::path::PathBuf;
use std::{fs, io};

use configuration::{Configuration, ConfigurationSettings};
use path_absolutize::Absolutize;
use thiserror::Error;
use tracing::level_filters::LevelFilter;
use usage::{Args, Subcommands, ValueEnum};

mod diagnostic;

/// Supply the terminal width to help rendering unless explicitly overridden.
///
/// # Safety
///
/// Call only during single-threaded startup, before starting any worker threads.
pub unsafe fn initialize_terminal_width() {
    if std::env::var_os("COLUMNS").is_none()
        && let Some((_, columns)) = console::Term::stdout().size_checked()
    {
        // SAFETY: The caller guarantees single-threaded startup.
        unsafe { std::env::set_var("COLUMNS", columns.to_string()) };
    }
}

fn absolute_path(value: PathBuf) -> io::Result<PathBuf> {
    value.absolutize().map(Cow::into_owned)
}

#[derive(Debug, usage::Cli)]
#[usage(
    bin = "iris",
    about = env!("CARGO_PKG_DESCRIPTION"),
    version = crate::VERSION,
    unknown_flags = "error",
    args_override_self = false
)]
pub struct Program {
    /// Print log path.
    #[usage(long)]
    pub log_file: bool,
    #[usage(subcommand)]
    pub command: Command,
}

impl Program {
    pub fn into_command(self) -> io::Result<Command> {
        let mut command = self.command;
        match &mut command {
            Command::Build(options) => options.build.normalize_paths()?,
            Command::Watch(options) => options.normalize_paths()?,
            Command::Run(options) => options.build.normalize_paths()?,
            Command::Test(options) => options.build.normalize_paths()?,
            Command::Compile(options) => {
                options.build.output = absolute_path(std::mem::take(&mut options.build.output))?;
                for package in &mut options.packages {
                    *package = absolute_path(std::mem::take(package))?;
                }
            }
            Command::Docs(options) => {
                options.output = absolute_path(std::mem::take(&mut options.output))?;
                options.spago_project =
                    options.spago_project.take().map(absolute_path).transpose()?;
                for package in &mut options.packages {
                    *package = absolute_path(std::mem::take(package))?;
                }
                if let Some(DocsCommand::TypeScript(options)) = &mut options.command {
                    options.output = absolute_path(std::mem::take(&mut options.output))?;
                }
            }
            Command::New(_) | Command::Add(_) | Command::Lsp(_) => {}
        }
        Ok(command)
    }
}

#[derive(Debug, Subcommands)]
pub enum Command {
    /// Create a Spago project in the current directory.
    #[usage(help_heading = "Project commands")]
    New(NewOptions),
    /// Add dependencies to a Spago package.
    #[usage(help_heading = "Project commands")]
    Add(AddOptions),
    /// Build a Spago workspace or package.
    #[usage(help_heading = "Project commands")]
    Build(ProjectBuildCommandOptions),
    /// Build a Spago workspace or package and rebuild when inputs change.
    #[usage(help_heading = "Project commands")]
    Watch(ProjectBuildOptions),
    /// Run the language server.
    #[usage(help_heading = "Project commands")]
    Lsp(LspOptions),
    /// Build and run a Spago package with Node.js.
    #[usage(help_heading = "Project commands")]
    Run(RunOptions),
    /// Build and test one or more Spago packages with Node.js.
    #[usage(help_heading = "Project commands")]
    Test(TestOptions),
    /// Compile PureScript modules to JavaScript (experimental).
    #[usage(help_heading = "Legacy commands")]
    Compile(CompileOptions),
    /// Documentation utilities.
    #[usage(help_heading = "Documentation commands")]
    Docs(DocsOptions),
}

#[derive(Debug, Args)]
pub struct LoggingOptions {
    /// Log level for the query engine.
    #[usage(
        long,
        value_name = "LevelFilter",
        default = "off",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    pub query_log: LevelFilter,

    /// Log level for the type checker.
    #[usage(
        long,
        value_name = "LevelFilter",
        default = "off",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    pub checking_log: LevelFilter,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct LspOptions {
    #[usage(flatten)]
    pub logging: LoggingOptions,

    #[usage(long)]
    pub stdio: bool,

    /// Log level for the language server.
    #[usage(
        long,
        value_name = "LevelFilter",
        default = "info",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    pub lsp_log: LevelFilter,

    /// Language server configuration as a JSON object, read once at startup.
    #[usage(long, value_name = "JSON", conflicts = "--config-file")]
    pub config: Option<String>,

    /// Language server configuration file, relative to the working directory.
    #[usage(long, value_name = "PATH", conflicts = "--config")]
    pub config_file: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("failed to read configuration file {}: {error}", path.display())]
    ReadFile { path: PathBuf, error: io::Error },
    #[error("invalid configuration in {input}: {error}")]
    InvalidJson { input: String, content: String, error: serde_json::Error },
}

impl LspOptions {
    pub fn configuration(&self) -> Result<Configuration, ConfigurationError> {
        let (input, content) = if let Some(path) = &self.config_file {
            let content = fs::read_to_string(path)
                .map_err(|error| ConfigurationError::ReadFile { path: path.clone(), error })?;
            (path.display().to_string(), Cow::Owned(content))
        } else if let Some(content) = &self.config {
            ("--config".to_string(), Cow::Borrowed(content.as_str()))
        } else {
            return Ok(Configuration::default());
        };
        let settings = serde_json::from_str::<Option<ConfigurationSettings>>(&content)
            .map_err(|error| ConfigurationError::InvalidJson {
                input,
                content: content.into_owned(),
                error,
            })?
            .unwrap_or_default();
        Ok(settings.apply_to(&Configuration::default()))
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct CompileOptions {
    #[usage(flatten)]
    pub build: BuildOptions,

    /// Package folder to compile.
    #[usage(name = "package", long = "package", value_name = "DIR")]
    pub packages: Vec<PathBuf>,

    /// PureScript source paths or glob patterns.
    #[usage(value_name = "INPUT", required_unless = "--package")]
    pub inputs: Vec<PathBuf>,

    /// Code generation targets requested by build tools.
    #[usage(long, value_name = "TARGETS")]
    pub codegen: Option<String>,

    /// Emit a Spago-compatible JSON result.
    ///
    /// Full structured JSON diagnostics are not yet supported.
    #[usage(long)]
    pub json_errors: bool,
}

#[derive(Debug, Args)]
pub struct BuildOptions {
    #[usage(flatten)]
    pub logging: LoggingOptions,

    /// Output directory for compiled modules.
    #[usage(short, long, value_name = "DIR", default = "output")]
    pub output: PathBuf,

    /// Suppress build progress output.
    #[usage(short, long)]
    pub quiet: bool,

    /// When to use colors in human-readable diagnostics.
    #[usage(long, value_enum, default = "auto")]
    pub color: ColorChoice,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct NewOptions {
    /// Package name. Defaults to the current directory name.
    #[usage(long, value_name = "NAME")]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct ProjectBuildOptions {
    #[usage(flatten)]
    pub logging: LoggingOptions,

    /// Workspace package to build.
    #[usage(short, long, value_name = "NAME")]
    pub package: Option<String>,

    /// Output directory for compiled modules. Defaults to output in the workspace root.
    #[usage(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Suppress build progress output.
    #[usage(short, long)]
    pub quiet: bool,

    /// When to use colors in human-readable diagnostics.
    #[usage(long, value_enum, default = "auto")]
    pub color: ColorChoice,
}

impl ProjectBuildOptions {
    fn normalize_paths(&mut self) -> io::Result<()> {
        self.output = self.output.take().map(absolute_path).transpose()?;
        Ok(())
    }
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct ProjectBuildCommandOptions {
    #[usage(flatten)]
    pub build: ProjectBuildOptions,

    /// Write JavaScript output even when compilation reports errors.
    #[usage(long)]
    pub resilient: bool,

    /// Suppress compiler warnings and errors without hiding build progress.
    #[usage(long)]
    pub no_diagnostics: bool,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct AddOptions {
    /// Workspace package whose dependencies should change.
    #[usage(short, long, value_name = "NAME")]
    pub package: Option<String>,

    /// Add packages as test dependencies.
    #[usage(long)]
    pub test: bool,

    /// Packages to add.
    #[usage(value_name = "DEPENDENCY", required = true)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct RunOptions {
    #[usage(flatten)]
    pub build: ProjectBuildOptions,

    /// Module containing the program entry point.
    #[usage(long, value_name = "MODULE")]
    pub main: Option<String>,

    /// Arguments passed to the program.
    #[usage(double_dash = "required")]
    pub arguments: Vec<String>,
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct TestOptions {
    #[usage(flatten)]
    pub build: ProjectBuildOptions,

    /// Module containing the test entry point.
    #[usage(long, value_name = "MODULE")]
    pub main: Option<String>,

    /// Arguments passed to each test program.
    #[usage(double_dash = "required")]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Args)]
#[usage(subcommand_negates_reqs = true, args_override_self = false)]
pub struct DocsOptions {
    #[usage(flatten)]
    pub logging: LoggingOptions,
    /// Log level for the documentation tool.
    #[usage(
        long,
        value_name = "LEVEL",
        default = "info",
        choices("off", "error", "warn", "info", "debug", "trace")
    )]
    pub docs_log: LevelFilter,
    #[usage(subcommand)]
    pub command: Option<DocsCommand>,
    /// Output directory for the generated documentation.
    #[usage(long, value_name = "DIR", default = "docs")]
    pub output: PathBuf,
    /// Suppress documentation progress output.
    #[usage(short, long)]
    pub quiet: bool,
    /// Spago project directory containing spago.lock.
    #[usage(long, value_name = "DIR", conflicts = "--package")]
    pub spago_project: Option<PathBuf>,
    /// Package folder to document.
    #[usage(
        name = "package",
        long = "package",
        value_name = "DIR",
        required_unless = "--spago-project"
    )]
    pub packages: Vec<PathBuf>,
}

#[derive(Debug, Subcommands)]
pub enum DocsCommand {
    /// Generate TypeScript declarations for the documentation JSON schema.
    #[usage(name = "typescript")]
    TypeScript(DocsTypeScriptOptions),
}

#[derive(Debug, Args)]
#[usage(args_override_self = false)]
pub struct DocsTypeScriptOptions {
    /// Output directory for the generated TypeScript schema.
    #[usage(long, value_name = "DIR", default = "src-generated")]
    pub output: PathBuf,
}

impl Default for LoggingOptions {
    fn default() -> LoggingOptions {
        LoggingOptions { query_log: LevelFilter::OFF, checking_log: LevelFilter::OFF }
    }
}

impl Default for LspOptions {
    fn default() -> LspOptions {
        LspOptions {
            logging: LoggingOptions::default(),
            stdio: false,
            lsp_log: LevelFilter::INFO,
            config: None,
            config_file: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;
    use usage::diagnostic::Code;

    fn parse(argv: Vec<&str>) -> Program {
        let argv: Vec<_> = argv.into_iter().map(OsStr::new).collect();
        Program::try_parse_from(&argv).unwrap()
    }

    fn error_kind(argv: Vec<&str>) -> Code {
        let argv: Vec<_> = argv.into_iter().map(OsStr::new).collect();
        let error = Program::try_parse_from(&argv).unwrap_err();
        usage::diagnostic::report(Program::spec(), &argv[1..], &error).code
    }

    fn docs(args: &[&str]) -> DocsOptions {
        let mut argv = vec!["iris", "docs"];
        argv.extend(args);
        let program = parse(argv);
        match program.into_command().unwrap() {
            Command::Docs(options) => options,
            _ => unreachable!("parsed command was not `docs`"),
        }
    }

    fn compile(args: &[&str]) -> CompileOptions {
        let mut argv = vec!["iris", "compile"];
        argv.extend(args);
        let program = parse(argv);
        match program.into_command().unwrap() {
            Command::Compile(options) => options,
            _ => unreachable!("parsed command was not `compile`"),
        }
    }

    fn compile_error_kind(args: &[&str]) -> Code {
        let mut argv = vec!["iris", "compile"];
        argv.extend(args);
        error_kind(argv)
    }

    fn build(args: &[&str]) -> ProjectBuildCommandOptions {
        let mut argv = vec!["iris", "build"];
        argv.extend(args);
        let program = parse(argv);
        match program.into_command().unwrap() {
            Command::Build(options) => options,
            _ => unreachable!("parsed command was not `build`"),
        }
    }

    fn watch(args: &[&str]) -> ProjectBuildOptions {
        let mut argv = vec!["iris", "watch"];
        argv.extend(args);
        let program = parse(argv);
        match program.into_command().unwrap() {
            Command::Watch(options) => options,
            _ => unreachable!("parsed command was not `watch`"),
        }
    }

    fn docs_error_kind(args: &[&str]) -> Code {
        let mut argv = vec!["iris", "docs"];
        argv.extend(args);
        error_kind(argv)
    }

    fn typescript(args: &[&str]) -> DocsTypeScriptOptions {
        match docs(args).command {
            Some(DocsCommand::TypeScript(options)) => options,
            _ => unreachable!("parsed command was not `typescript`"),
        }
    }

    fn current_directory() -> PathBuf {
        std::env::current_dir().unwrap()
    }

    fn current_directory_path(path: impl AsRef<Path>) -> PathBuf {
        current_directory().join(path)
    }

    #[test]
    fn compile_accepts_spago_arguments() {
        let options = compile(&[
            "--codegen",
            "corefn,docs,js,sourcemaps",
            "--json-errors",
            "--quiet",
            "--color",
            "always",
            "src/**/*.purs",
            ".spago/p/prelude-6.0.2/src/**/*.purs",
        ]);

        assert_eq!(options.build.output, current_directory_path("output"));
        assert_eq!(options.codegen.as_deref(), Some("corefn,docs,js,sourcemaps"));
        assert!(options.json_errors);
        assert!(options.build.quiet);
        assert_eq!(options.build.color, ColorChoice::Always);
        assert_eq!(
            options.inputs,
            vec![
                PathBuf::from("src/**/*.purs"),
                PathBuf::from(".spago/p/prelude-6.0.2/src/**/*.purs"),
            ]
        );
    }

    #[test]
    fn watch_accepts_project_build_arguments() {
        let options =
            watch(&["--package", "application", "--output", "dist", "--quiet", "--color", "never"]);

        assert_eq!(options.package.as_deref(), Some("application"));
        assert_eq!(options.output, Some(current_directory_path("dist")));
        assert!(options.quiet);
        assert_eq!(options.color, ColorChoice::Never);
    }

    #[test]
    fn build_accepts_resilient_output() {
        let options = build(&["--resilient", "--no-diagnostics"]);

        assert!(options.resilient);
        assert!(options.no_diagnostics);
        assert!(!build(&[]).resilient);
        assert!(!build(&[]).no_diagnostics);
    }

    #[test]
    fn compile_accepts_package_folders_without_inputs() {
        let options =
            compile(&["--package", "packages/effect", "--package", "packages/prelude", "--quiet"]);

        assert_eq!(
            options.packages,
            vec![
                current_directory_path("packages/effect"),
                current_directory_path("packages/prelude"),
            ]
        );
        assert!(options.inputs.is_empty());
        assert!(options.build.quiet);
    }

    #[test]
    fn compile_requires_inputs_or_a_package() {
        insta::assert_debug_snapshot!(compile_error_kind(&[]), @"MissingRequired");
    }

    #[test]
    fn single_package_folder() {
        let options = docs(&["--package", "packages/effect", "--quiet"]);

        assert_eq!(options.packages, vec![current_directory_path("packages/effect")]);
        assert!(options.quiet);
    }

    #[test]
    fn repeated_packages_keep_order() {
        let options = docs(&["--package", "packages/effect", "--package", "packages/prelude"]);

        assert_eq!(
            options.packages,
            vec![
                current_directory_path("packages/effect"),
                current_directory_path("packages/prelude"),
            ]
        );
    }

    #[test]
    fn spago_project_replaces_package_specs() {
        let options = docs(&["--spago-project", "."]);

        assert_eq!(options.spago_project, Some(current_directory()));
        assert!(options.packages.is_empty());
    }

    #[test]
    fn spago_project_conflicts_with_package_specs() {
        insta::assert_debug_snapshot!(
            docs_error_kind(&["--spago-project", ".", "--package", "packages/effect"]),
            @"ConflictingFlags"
        );
    }

    #[test]
    fn later_flags_are_not_consumed_as_package_paths() {
        let options = docs(&["--package", "packages/effect", "--output", "out"]);

        assert_eq!(options.output, current_directory_path("out"));
        assert_eq!(options.packages, vec![current_directory_path("packages/effect")]);
    }

    #[test]
    fn relative_paths_are_normalised() {
        let options = docs(&["--spago-project", "./spago/..", "--output", "./generated/../docs"]);

        assert_eq!(options.spago_project, Some(current_directory()));
        assert_eq!(options.output, current_directory_path("docs"));
    }

    #[test]
    fn absolute_paths_are_not_resolved_from_the_current_directory() {
        let output = std::env::temp_dir().join("iris-docs-output");
        let options = docs(&["--package", "packages/effect", "--output", output.to_str().unwrap()]);

        assert_eq!(options.output, output);
    }

    #[test]
    fn missing_package_path_is_rejected() {
        insta::assert_debug_snapshot!(docs_error_kind(&["--package"]), @"MissingFlagValue");
    }

    #[test]
    fn package_is_required() {
        insta::assert_debug_snapshot!(docs_error_kind(&[]), @"MissingRequired");
    }

    #[test]
    fn typescript_has_default_output() {
        let options = typescript(&["typescript"]);

        assert_eq!(options.output, current_directory_path("src-generated"));
    }
}
