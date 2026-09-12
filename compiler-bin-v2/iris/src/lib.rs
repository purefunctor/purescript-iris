mod cli;
mod logging;

pub(crate) const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const VERSION: &str = env!("IRIS_VERSION");

pub fn run() -> i32 {
    let program = cli::Program::parse_with_diagnostics();
    match program.command {
        cli::Command::New(options) => match iris_package_manager::create(options.into_config()) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        cli::Command::Add(options) => match iris_package_manager::add(options.into_config()) {
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
