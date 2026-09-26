mod cli;
mod logging;

use iris_watch_server::discovery::OutputLock;

pub(crate) const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const VERSION: &str = env!("IRIS_VERSION");

pub fn run() -> i32 {
    let program = cli::Program::parse_with_diagnostics();
    match program.command {
        cli::Command::New(options) => match iris_package::create(options.into_config()) {
            Ok(package) => {
                println!(
                    "Created package `{}` with package set {}.\nRun `iris build` to get started.",
                    package.name, package.package_set
                );
                0
            }
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        cli::Command::Add(options) => match iris_package::add(options.into_config()) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        cli::Command::Build(options) => match iris_build::build(options.into_config()) {
            Ok(()) => 0,
            Err(error) => {
                if !error.diagnostics_were_suppressed() {
                    eprintln!("{error}");
                }
                1
            }
        },
        cli::Command::Watch(options) => {
            let config = options.into_config();
            // Locking before Spago runs stops a second watcher before it fetches and builds.
            let lock = iris_build::resolve_output(config.project.output.as_deref())
                .map_err(|error| error.to_string())
                .and_then(|output| OutputLock::acquire(&output).map_err(|error| error.to_string()));
            let lock = match lock {
                Ok(lock) => lock,
                Err(error) => {
                    eprintln!("{error}");
                    return 1;
                }
            };
            let project = match iris_build::prepare_project(config.project) {
                Ok(project) => project,
                Err(error) => {
                    eprintln!("{error}");
                    return 1;
                }
            };
            match iris_watch::watch(project, lock, config.watch) {
                Ok(status) => status,
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        }
        cli::Command::Lsp(options) => {
            if let Err(error) = logging::start(options.into_config()) {
                eprintln!("error: failed to start logging: {error}");
                return 1;
            }
            let server = iris_lsp::ServerConfig {
                name: PACKAGE_NAME.to_string(),
                version: VERSION.to_string(),
            };
            match iris_lsp::start(server) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("error: language server failed: {error}");
                    1
                }
            }
        }
        cli::Command::Run { options } => match iris_build::run(options.into_config()) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                error.exit_code()
            }
        },
        cli::Command::Test { options } => match iris_build::test(options.into_config()) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                error.exit_code()
            }
        },
    }
}
