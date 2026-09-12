mod cli;
mod logging;

pub(crate) const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const VERSION: &str = env!("IRIS_VERSION");

pub fn run() -> i32 {
    let program = cli::Program::parse_with_diagnostics();
    match program.command {
        cli::Command::New(options) => match iris_package::create(options.into_config()) {
            Ok(()) => 0,
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
            let project = match iris_build::prepare_project(config.project) {
                Ok(project) => project,
                Err(error) => {
                    eprintln!("{error}");
                    return 1;
                }
            };
            if let Err(error) = iris_watch::watch(project, config.watch) {
                eprintln!("{error}");
                return 1;
            }
            0
        }
        cli::Command::Lsp(options) => {
            let config = match options.into_config() {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };
            if let Err(error) = logging::start(config.logging) {
                eprintln!("error: failed to start logging: {error}");
                return 1;
            }
            let server = iris_lsp::ServerConfig {
                configuration: config.configuration,
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
    }
}
