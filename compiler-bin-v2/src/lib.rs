pub mod build;
mod cli;

pub(crate) const VERSION: &str = env!("IRIS_VERSION");

pub fn run() -> i32 {
    let program = cli::Program::parse_with_diagnostics();
    match build::project::build(program.command.into_config()) {
        Ok(()) => 0,
        Err(error) => {
            if !error.diagnostics_were_suppressed() {
                eprintln!("{error}");
            }
            1
        }
    }
}
