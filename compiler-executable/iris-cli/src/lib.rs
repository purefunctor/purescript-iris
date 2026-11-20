mod cli;
mod format;
mod logging;
mod skills;

use std::env;

use iris_self_update::{Availability, Update};
use iris_watch_query::RefactorChanges;
use iris_watch_server::client::{self, ClientError};
use iris_watch_server::discovery::OutputLock;
use iris_watch_server::protocol::Response;

pub(crate) const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub(crate) const VERSION: &str = env!("IRIS_VERSION");

pub fn run() -> i32 {
    let program = cli::Program::parse_with_diagnostics();
    match program.command {
        cli::Command::Format(options) => format::run(options),
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
            if let Some(command) = options.take_command() {
                return match command {
                    cli::WatchCommand::Query(options) => query(options),
                    cli::WatchCommand::Refactor(options) => refactor(options),
                    cli::WatchCommand::Mcp(options) => mcp(options),
                };
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
        cli::Command::Installation(options) => match options.command {
            cli::InstallationCommand::Update(options) => self_update(options),
        },
    }
}

/// Exit status of `iris self update --check` when another release is available, as
/// `dnf check-update` reports available updates, so scripts can tell it apart from a failure.
const UPDATE_AVAILABLE: i32 = 100;

fn self_update(options: cli::UpdateOptions) -> i32 {
    if options.check {
        return match iris_self_update::check(options.into_config()) {
            Ok(Availability::UpToDate(release)) => {
                println!("Iris {release} is up to date.");
                0
            }
            Ok(Availability::Available(release)) => {
                println!("Iris {release} is available; this is Iris {VERSION}.");
                UPDATE_AVAILABLE
            }
            Err(error) => {
                eprintln!("{error}");
                1
            }
        };
    }
    match iris_self_update::update(options.into_config()) {
        Ok(Update::AlreadyInstalled(release)) => {
            println!("Iris {release} is already installed.");
            0
        }
        Ok(Update::Installed(installed)) => {
            let release = installed.release;
            println!(
                "Replaced Iris {VERSION} with Iris {release} at {}.",
                installed.executable.display()
            );
            println!("Restart `iris watch` and editor language servers to use Iris {release}.");
            if !installed.has_self_update {
                eprintln!(
                    "warning: Iris {release} cannot update itself; run the installer to update it."
                );
            }
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
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
            eprintln!("{error}");
            return NO_WATCHER;
        }
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    if let Some(warning) = discovery.version_warning(VERSION) {
        eprintln!("warning: {warning}");
    }
    if options.json {
        println!("{}", response.to_line().trim_end());
    }
    match response {
        Response::Result { value } => {
            if options.json {
                return 0;
            }
            match iris_watch_query::render(&query, &value) {
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
        Response::Error { message } => {
            if !options.json {
                eprintln!("{message}");
            }
            1
        }
        Response::Cancelled => {
            unreachable!("invariant violated: cancelled queries are sent again")
        }
    }
}

fn mcp(options: cli::McpOptions) -> i32 {
    let output = match iris_build::resolve_output(options.output.as_deref()) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    match iris_watch_mcp::serve(output, VERSION.to_string()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: MCP server failed: {error}");
            1
        }
    }
}

fn refactor(options: cli::RefactorOptions) -> i32 {
    let explicit_module = options.refactor.has_module();
    let query = options.refactor.into_query();
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
            eprintln!("{error}");
            return NO_WATCHER;
        }
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    if let Some(warning) = discovery.version_warning(VERSION) {
        eprintln!("warning: {warning}");
    }

    match response {
        Response::Result { value } => {
            let raw_json = options.json.then(|| Response::Result { value: value.clone() });
            let answer = match iris_watch_query::parse_refactor_answer(value) {
                Ok(answer) => answer,
                Err(error) => {
                    eprintln!("the watcher sent an answer this Iris cannot read: {error}");
                    return 1;
                }
            };
            let explicit_module_has_errors =
                explicit_module && answer.is_blocked_by_module_errors();
            let exit_code = i32::from(explicit_module_has_errors);

            if options.preview && options.json {
                if let Some(raw_json) = raw_json {
                    println!("{}", raw_json.to_line().trim_end());
                }
                return exit_code;
            }
            let root = match discovery.root {
                Some(root) => root,
                None => match env::current_dir() {
                    Ok(directory) => directory,
                    Err(error) => {
                        eprintln!("could not determine working directory: {error}");
                        return 1;
                    }
                },
            };

            let changes = match RefactorChanges::prepare(&root, &answer) {
                Ok(changes) => changes,
                Err(error) => {
                    eprintln!("{error}");
                    return 1;
                }
            };

            if options.preview {
                print!("{}", changes.patch());
                if let Some(skipped) = iris_watch_query::render_refactor_skipped(&answer) {
                    eprintln!("{skipped}");
                }
                return exit_code;
            }

            if let Err(error) = changes.write() {
                eprintln!("{error}");
                return 1;
            }

            if let Some(raw_json) = raw_json {
                println!("{}", raw_json.to_line().trim_end());
            } else if explicit_module_has_errors {
                eprintln!("{}", iris_watch_query::render_refactor_summary(&answer));
            } else {
                println!("{}", iris_watch_query::render_refactor_summary(&answer));
            }
            exit_code
        }
        Response::Error { message } => {
            if options.json {
                println!("{}", Response::Error { message }.to_line().trim_end());
            } else {
                eprintln!("{message}");
            }
            1
        }
        Response::Cancelled => {
            unreachable!("invariant violated: cancelled queries are sent again")
        }
    }
}
