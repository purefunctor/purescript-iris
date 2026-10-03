use std::collections::BTreeSet;
use std::fs::{self, File, Metadata};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::cli::FormatOptions;

struct FormattedFile {
    path: PathBuf,
    source: String,
    formatted: String,
    metadata: Metadata,
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
    if options.write && options.check {
        return Err("--write and --check cannot be used together".to_string());
    }

    let reads_standard_input = options.paths.is_empty()
        || (options.paths.len() == 1 && options.paths[0].as_os_str() == "-");
    if options.paths.iter().any(|path| path.as_os_str() == "-") && !reads_standard_input {
        return Err("standard input cannot be mixed with file paths".to_string());
    }
    if reads_standard_input {
        if options.write {
            return Err("--write cannot be used with standard input".to_string());
        }
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| format!("failed to read standard input: {error}"))?;
        let formatted = iris_format::format_module(&source)
            .map_err(|error| format!("failed to format standard input: {error}"))?;
        if options.check {
            return Ok(i32::from(formatted != source));
        }
        print!("{formatted}");
        return Ok(0);
    }
    if !options.write && !options.check && options.paths.len() != 1 {
        return Err("preview mode accepts exactly one file".to_string());
    }

    let mut files = Vec::with_capacity(options.paths.len());
    let mut seen = BTreeSet::new();
    for path in options.paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
        if options.write && metadata.file_type().is_symlink() {
            return Err(format!("refusing to rewrite symlink {}", path.display()));
        }
        let metadata = if metadata.file_type().is_symlink() {
            fs::metadata(&path)
                .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
        } else {
            metadata
        };
        if !metadata.is_file() {
            return Err(format!("{} is not a regular file", path.display()));
        }
        let identity = fs::canonicalize(&path)
            .map_err(|error| format!("failed to resolve {}: {error}", path.display()))?;
        if !seen.insert(identity) {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let formatted = iris_format::format_module(&source)
            .map_err(|error| format!("failed to format {}: {error}", path.display()))?;
        files.push(FormattedFile { path, source, formatted, metadata });
    }

    if options.check {
        let mut changed = false;
        for file in &files {
            if file.source != file.formatted {
                println!("{}", file.path.display());
                changed = true;
            }
        }
        return Ok(i32::from(changed));
    }
    if options.write {
        for file in files.iter().filter(|file| file.source != file.formatted) {
            write_file(file)?;
        }
        return Ok(0);
    }

    print!("{}", files[0].formatted);
    Ok(0)
}

fn write_file(file: &FormattedFile) -> Result<(), String> {
    let parent = file.path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        format!("failed to create temporary file beside {}: {error}", file.path.display())
    })?;
    temporary.as_file().set_permissions(file.metadata.permissions()).map_err(|error| {
        format!("failed to preserve permissions for {}: {error}", file.path.display())
    })?;
    temporary
        .write_all(file.formatted.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| {
            format!("failed to write temporary file for {}: {error}", file.path.display())
        })?;

    let current = read_regular_file(&file.path)?;
    if current != file.source {
        return Err(format!("{} changed while it was being formatted", file.path.display()));
    }
    let current_metadata = fs::symlink_metadata(&file.path)
        .map_err(|error| format!("failed to inspect {}: {error}", file.path.display()))?;
    if current_metadata.file_type().is_symlink() {
        return Err(format!("refusing to rewrite symlink {}", file.path.display()));
    }
    temporary
        .persist(&file.path)
        .map_err(|error| format!("failed to replace {}: {}", file.path.display(), error.error))?;
    Ok(())
}

fn read_regular_file(path: &Path) -> Result<String, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to reopen {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is no longer a regular file", path.display()));
    }
    let mut source = String::new();
    io::BufReader::new(file)
        .read_to_string(&mut source)
        .map_err(|error| format!("failed to reread {}: {error}", path.display()))?;
    Ok(source)
}
