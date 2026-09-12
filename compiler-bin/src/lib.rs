use tracing::level_filters::LevelFilter;

pub mod cli;
mod compilation;
pub mod compile;
pub mod docs;
pub mod logging;
pub mod lsp;
mod package;
mod progress;
mod project;
pub mod walk;
mod watch;
mod workspace;

pub(crate) const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const VERSION: &str = env!("IRIS_VERSION");

pub fn run() {
    let program = cli::Program::parse_with_diagnostics();

    if program.log_file {
        eprintln!("Log file: {:?}", logging::temporary_log_file());
    }

    let command = program.into_command().unwrap_or_else(|error| {
        eprintln!("error: failed to resolve command paths: {error}");
        std::process::exit(2);
    });

    match command {
        cli::Command::Lsp(options) => {
            let configuration = options.configuration().unwrap_or_else(|error| {
                eprint!("{}", error.render());
                std::process::exit(2);
            });
            logging::start(logging::LoggingFilters {
                query_log: options.logging.query_log,
                checking_log: options.logging.checking_log,
                lsp_log: options.lsp_log,
                docs_log: LevelFilter::OFF,
            });
            lsp::start(configuration);
        }
        cli::Command::New(options) => project::start(project::new(options.name)),
        cli::Command::Build(options) => {
            start_project_logging(&options.build.logging);
            let resilience = if options.resilient {
                compile::Resilience::Resilient
            } else {
                compile::Resilience::Strict
            };
            let mut config = project_build_config(options.build);
            config.diagnostics = !options.no_diagnostics;
            project::start(project::build(config, resilience));
        }
        cli::Command::Add(options) => {
            project::start(project::add(project::AddProjectConfig {
                package: options.package,
                dependencies: options.dependencies,
                test_dependencies: options.test,
            }));
        }
        cli::Command::Run(options) => {
            start_project_logging(&options.build.logging);
            project::start(project::run(project::RunProjectConfig {
                build: project_build_config(options.build),
                main: options.main,
                arguments: options.arguments,
            }));
        }
        cli::Command::Test(options) => {
            start_project_logging(&options.build.logging);
            project::start(project::test(project::TestProjectConfig {
                build: project_build_config(options.build),
                main: options.main,
                arguments: options.arguments,
            }));
        }
        cli::Command::Compile(options) => {
            logging::start(logging::LoggingFilters {
                query_log: options.build.logging.query_log,
                checking_log: options.build.logging.checking_log,
                lsp_log: LevelFilter::OFF,
                docs_log: LevelFilter::OFF,
            });
            compile::start(compile::CompileConfig {
                output: options.build.output,
                inputs: options.inputs,
                packages: options.packages,
                json_errors: options.json_errors,
                quiet: options.build.quiet,
                color: options.build.color,
            });
        }
        cli::Command::Watch(options) => {
            start_project_logging(&options.logging);
            project::start(project::watch(project_build_config(options)));
        }
        cli::Command::Docs(options) => {
            logging::start(logging::LoggingFilters {
                query_log: options.logging.query_log,
                checking_log: options.logging.checking_log,
                lsp_log: LevelFilter::OFF,
                docs_log: options.docs_log,
            });
            match options.command {
                Some(cli::DocsCommand::TypeScript(options)) => {
                    docs::typescript(docs::TypeScriptConfig { output: options.output });
                }
                None => {
                    docs::start(docs::DocsConfig {
                        output: options.output,
                        spago_project: options.spago_project,
                        packages: options.packages,
                        quiet: options.quiet,
                    });
                }
            }
        }
    }
}

fn start_project_logging(options: &cli::LoggingOptions) {
    logging::start(logging::LoggingFilters {
        query_log: options.query_log,
        checking_log: options.checking_log,
        lsp_log: LevelFilter::OFF,
        docs_log: LevelFilter::OFF,
    });
}

fn project_build_config(options: cli::ProjectBuildOptions) -> project::BuildProjectConfig {
    project::BuildProjectConfig {
        package: options.package,
        output: options.output,
        quiet: options.quiet,
        color: options.color,
        diagnostics: true,
    }
}
