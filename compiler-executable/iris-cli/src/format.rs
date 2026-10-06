use std::collections::BTreeSet;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::cli::{FormatMode, FormatOptions};

struct Input {
    path: Option<PathBuf>,
    source: String,
    formatted: String,
}

pub fn run(options: FormatOptions) -> i32 {
    match execute(options) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("error: {error}");
            2
        }
    }
}

fn execute(options: FormatOptions) -> Result<i32, String> {
    let defaults = formatting::Config::default();
    let config = formatting::Config {
        line_width: options.width.unwrap_or(defaults.line_width),
        indent_width: options.indent.unwrap_or(defaults.indent_width),
        unicode: options.unicode,
    };
    config.validate().map_err(|error| error.to_string())?;

    let (paths, mode) = if options.files.is_empty() {
        let directory = env::current_dir().map_err(|error| error.to_string())?;
        let workspace =
            iris_build::Workspace::discover(&directory, None).map_err(|error| error.to_string())?;
        let files = workspace.source_files().map_err(|error| error.to_string())?;
        (files, Some(options.mode.unwrap_or(FormatMode::Write)))
    } else {
        (options.files, options.mode)
    };

    let mut files = paths.into_iter().collect::<BTreeSet<_>>();
    let has_stdin = files.remove(Path::new("-"));
    if matches!(mode, Some(FormatMode::Write)) && has_stdin {
        return Err("cannot use --write with standard input".into());
    }
    let input_count = files.len() + usize::from(has_stdin);
    if input_count > 1 && mode.is_none() {
        return Err("multiple files require --write or --check".into());
    }

    let mut inputs = Vec::with_capacity(input_count);
    if has_stdin {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| format!("could not read standard input: {error}"))?;
        let formatted = formatting::format_with_config(&source, &config)
            .map_err(|error| format!("standard input: {error}"))?;
        inputs.push(Input { path: None, source, formatted });
    }
    for path in files {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let formatted = formatting::format_with_config(&source, &config)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        inputs.push(Input { path: Some(path), source, formatted });
    }

    match mode {
        Some(FormatMode::Check) => {
            let mut dirty = false;
            for input in &inputs {
                if input.source != input.formatted {
                    match &input.path {
                        Some(path) => eprintln!("{}", path.display()),
                        None => eprintln!("-"),
                    }
                    dirty = true;
                }
            }
            Ok(i32::from(dirty))
        }
        Some(FormatMode::Write) => {
            // The symlink preflight is a convenience check, not a security boundary.
            for input in &inputs {
                let path = input.path.as_deref().expect("standard input was rejected");
                let metadata = fs::symlink_metadata(path)
                    .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
                if metadata.file_type().is_symlink() {
                    return Err(format!("refusing to replace symlink {}", path.display()));
                }
            }

            for input in &inputs {
                if input.source != input.formatted {
                    let path = input.path.as_deref().expect("standard input was rejected");
                    fs::write(path, &input.formatted)
                        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                }
            }
            Ok(0)
        }
        None => {
            if let Some(input) = inputs.first() {
                io::stdout()
                    .write_all(input.formatted.as_bytes())
                    .map_err(|error| format!("could not write standard output: {error}"))?;
            }
            Ok(0)
        }
    }
}
