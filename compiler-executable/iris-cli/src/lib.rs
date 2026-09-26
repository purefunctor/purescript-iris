mod cli;
mod logging;
mod skills;

use iris_watch_server::client::{self, ClientError};
use iris_watch_server::discovery::OutputLock;
use iris_watch_server::protocol::ResponseBody;

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
        cli::Command::Watch(mut options) => {
            if let Some(options) = options.take_query() {
                return query(options);
            }
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
        cli::Command::Skills(options) => match options.command {
            None | Some(cli::SkillsCommand::List {}) => {
                for skill in skills::SKILLS {
                    println!("{}\t{}", skill.name, skill.description());
                }
                0
            }
            Some(cli::SkillsCommand::Get(cli::SkillName { name })) => {
                if let Some(skill) = skills::Skill::find(&name) {
                    print!("{}", skill.content());
                    0
                } else {
                    let available = skills::SKILLS.iter().map(|skill| skill.name);
                    let available = available.collect::<Vec<_>>().join(", ");
                    eprintln!("no skill is named `{name}`; available skills: {available}");
                    1
                }
            }
        },
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

/// Exit status when no watcher is running, so scripts can tell it apart from a failed query.
const NO_WATCHER: i32 = 4;

fn query(options: cli::QueryOptions) -> i32 {
    let query = match options.query.into_query() {
        Ok(query) => query,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let output = match iris_build::resolve_output(options.output.as_deref()) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    let (discovery, response) = match client::query(&output, &query) {
        Ok(answer) => answer,
        Err(error @ ClientError::NoWatcher { .. }) => {
            eprintln!("{error}; start one with `iris watch`, or retry if it is starting");
            return NO_WATCHER;
        }
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    if discovery.version != VERSION {
        let executable = discovery.executable.as_ref().map_or_else(
            || "the watcher's `iris`".to_string(),
            |executable| executable.display().to_string(),
        );
        eprintln!(
            "warning: the watcher runs Iris {} and this is Iris {VERSION}; \
             use {executable} if answers look wrong",
            discovery.version
        );
    }
    if options.json {
        println!("{}", response.to_line().trim_end());
    }
    match response.body {
        ResponseBody::Result { value, .. } => {
            if options.json {
                return 0;
            }
            match iris_watch_query::render(&query, value) {
                Ok(text) => {
                    println!("{text}");
                    0
                }
                Err(error) => {
                    eprintln!("the watcher sent an answer this Iris cannot read: {error}");
                    1
                }
            }
        }
        ResponseBody::Error { message } => {
            if !options.json {
                eprintln!("{message}");
            }
            1
        }
        ResponseBody::Cancelled => {
            unreachable!("invariant violated: cancelled queries are sent again")
        }
    }
}
