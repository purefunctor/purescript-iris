use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

const DUMPED_FUNCTION: &str = "*measured_stage*";
const COLLECTED_FUNCTION: &str = "*collected_stage*";

const ALLOCATOR_PREFIXES: [&str; 3] = ["mi_", "_mi_", "__mi_"];

const ALLOCATOR_ENTRY_POINTS: [&str; 8] = [
    "mi_malloc",
    "mi_malloc_aligned",
    "mi_zalloc",
    "mi_zalloc_aligned",
    "mi_realloc",
    "mi_realloc_aligned",
    "mi_free",
    "mi_free_aligned",
];

pub struct Dump {
    pub path: PathBuf,
    pub part: u32,
    pub trigger: String,
    pub instructions: u64,
    pub allocator_instructions: u64,
}

pub fn run(output_dir: &Path, child_arguments: &[OsString]) -> Result<Vec<Dump>> {
    if output_dir.exists() {
        fs::remove_dir_all(output_dir)
            .with_context(|| format!("failed to clear {}", output_dir.display()))?;
    }
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let executable =
        std::env::current_exe().context("failed to locate the measuring executable")?;
    let log_path = output_dir.join("valgrind.log");
    let log = fs::File::create(&log_path)
        .with_context(|| format!("failed to create {}", log_path.display()))?;

    let mut out_file = OsString::from("--callgrind-out-file=");
    out_file.push(output_dir.join("callgrind.out"));

    let status = Command::new("valgrind")
        .arg("--tool=callgrind")
        .arg("--cache-sim=no")
        .arg("--branch-sim=no")
        .arg("--collect-atstart=no")
        .arg(format!("--toggle-collect={COLLECTED_FUNCTION}"))
        .arg(format!("--dump-after={DUMPED_FUNCTION}"))
        .arg(out_file)
        .arg(executable)
        .args(child_arguments)
        .env("RAYON_NUM_THREADS", "1")
        .env("MIMALLOC_PURGE_DELAY", "-1")
        .stdout(Stdio::null())
        .stderr(log)
        .status();

    let status = match status {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("valgrind is not installed; instruction counts need Valgrind on Linux")
        }
        Err(error) => return Err(error).context("failed to start valgrind"),
    };
    if !status.success() {
        bail!("the measured process failed with {status}; inspect {}", log_path.display());
    }

    let mut dumps = Vec::new();
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        let is_dump = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("callgrind.out"));
        if is_dump {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let dump = parse_dump(path.clone(), &content).with_context(|| {
                format!("failed to parse the Callgrind dump {}", path.display())
            })?;
            dumps.push(dump);
        }
    }

    dumps.retain(|dump| dump.trigger.starts_with("--dump-after"));
    dumps.sort_by_key(|dump| dump.part);
    Ok(dumps)
}

fn parse_dump(path: PathBuf, content: &str) -> Result<Dump> {
    let mut part = None;
    let mut trigger = None;
    let mut instructions = None;
    let mut allocator_instructions = 0;
    let mut names = HashMap::new();
    let mut caller = None;
    let mut callee = None;
    let mut call_pending = false;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("part: ") {
            part = Some(value.trim().parse()?);
        } else if let Some(value) = line.strip_prefix("desc: Trigger: ") {
            trigger = Some(value.trim().to_string());
        } else if let Some(value) =
            line.strip_prefix("totals: ").or_else(|| line.strip_prefix("summary: "))
        {
            let value = value.split_whitespace().next().unwrap_or("0");
            instructions = Some(value.parse()?);
        } else if let Some(value) = line.strip_prefix("fn=") {
            caller = resolve_name(&mut names, value);
        } else if let Some(value) = line.strip_prefix("cfn=") {
            callee = resolve_name(&mut names, value);
        } else if line.starts_with("calls=") {
            call_pending = true;
        } else if call_pending
            && line.starts_with(|first: char| first.is_ascii_digit() || "+-*".contains(first))
        {
            call_pending = false;
            let is_entry_point =
                callee.as_deref().is_some_and(|callee| ALLOCATOR_ENTRY_POINTS.contains(&callee));
            let is_inside_allocator = caller.as_deref().is_some_and(|caller| {
                ALLOCATOR_PREFIXES.iter().any(|prefix| caller.starts_with(prefix))
            });
            if is_entry_point && !is_inside_allocator {
                let cost = line.split_whitespace().nth(1).unwrap_or("0");
                allocator_instructions += cost.parse::<u64>()?;
            }
        }
    }
    let part = part.unwrap_or(0);
    let trigger = trigger.context("the dump has no trigger line")?;
    let instructions = instructions.context("the dump has no totals line")?;
    Ok(Dump { path, part, trigger, instructions, allocator_instructions })
}

fn resolve_name(names: &mut HashMap<String, String>, value: &str) -> Option<String> {
    let Some(rest) = value.strip_prefix('(') else { return Some(value.to_string()) };
    let (id, name) = rest.split_once(')')?;
    let name = name.trim();
    if !name.is_empty() {
        names.insert(id.to_string(), name.to_string());
    }
    names.get(id).cloned()
}
