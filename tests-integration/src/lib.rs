pub mod fixtures;
pub mod generated;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use building::QueryEngine;
use files::{FileId, Files, ForeignFiles, ForeignSourceKind};
use glob::glob;
use prim_constants::MODULE_MAP;
use tempfile::TempDir;
use url::Url;

use crate::fixtures::FixtureResult;

static PRIM_DIRECTORY: LazyLock<TempDir> =
    LazyLock::new(|| TempDir::new().expect("invariant violated: failed to create PRIM_DIRECTORY"));

fn configure_materialized_prim(engine: &QueryEngine, files: &mut Files) {
    for (name, content) in MODULE_MAP {
        let path = PRIM_DIRECTORY.path().join(format!("{name}.purs"));
        fs::write(&path, content).expect("invariant violated: failed to materialize Prim module");

        let uri = Url::from_file_path(path)
            .expect("invariant violated: failed to create Prim module file URL");
        let id = files.insert(uri.as_str(), *content);

        engine.set_content(id, *content);
        engine.set_module_file(name, id);
    }
}

fn load_file(
    engine: &mut QueryEngine,
    files: &mut Files,
    foreign_files: &mut ForeignFiles,
    path: &Path,
    replaceable: &HashSet<FileId>,
    replacements: &mut BTreeMap<String, String>,
) -> FixtureResult<FileId> {
    let url =
        Url::from_file_path(path).map_err(|()| std::io::Error::other("invalid source path"))?;
    let file = fs::read_to_string(path)?;
    let file = file.replace("\r\n", "\n");

    let uri = url.to_string();
    let id = files.insert(uri, file);
    let content = files.content(id);

    engine.set_content(id, content.clone());
    engine.set_dialect(id, files.dialect(id));
    let Ok((parsed, _)) = engine.parsed(id) else {
        return Ok(id);
    };

    if let Some(name) = parsed.module_name(&content) {
        if let Some(previous) = engine.module_file(&name)
            && (!replaceable.contains(&previous) || replacements.remove(name.as_str()).is_none())
        {
            let message = format!(
                "duplicate module {name}: {} and {}; \
                 declare intentional registry replacements in replacements.json",
                files.path(previous),
                path.display(),
            );
            return Err(std::io::Error::other(message).into());
        }
        engine.set_module_file(&name, id);
    }

    for kind in ForeignSourceKind::ALL {
        let foreign_path = path.with_extension(kind.extension());
        if !foreign_path.is_file() {
            continue;
        }
        let foreign_url = Url::from_file_path(&foreign_path)
            .map_err(|()| std::io::Error::other("invalid foreign source path"))?;
        let foreign_content = fs::read_to_string(foreign_path)?;
        let foreign_content = foreign_content.replace("\r\n", "\n");
        let foreign_id = foreign_files.insert(kind, foreign_url.as_str(), foreign_content);
        engine.set_foreign_content(foreign_id, foreign_files.content(foreign_id));
        engine.set_foreign_file(id, foreign_id);
    }
    Ok(id)
}

fn load_folder(folder: &Path) -> FixtureResult<Vec<PathBuf>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let packages = manifest.join(folder);
    let root = glob::Pattern::escape(&packages.to_string_lossy());
    let mut sources = Vec::new();
    for extension in ["purs", "iris"] {
        let pattern = format!("{root}/**/*.{extension}");
        sources.extend(glob(&pattern)?.collect::<Result<Vec<_>, _>>()?);
    }
    Ok(sources)
}

pub fn load_compiler(folder: &Path) -> FixtureResult<(QueryEngine, Files)> {
    let loaded = load_fixture(folder)?;
    Ok((loaded.engine, loaded.files))
}

pub struct LoadedFixture {
    pub engine: QueryEngine,
    pub files: Files,
    pub fixture_files: HashSet<FileId>,
}

pub fn load_fixture(folder: &Path) -> FixtureResult<LoadedFixture> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let packages = if folder.starts_with("fixtures/compiler/") {
        tests_support::prepared_sources(
            manifest.join("packages.json"),
            manifest.join("../target/integration-packages"),
        )
        .map_err(|error| {
            std::io::Error::other(format!(
                "{error:#}; run `just integration-prepare` before running integration tests"
            ))
        })?
    } else {
        Vec::new()
    };
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    let mut foreign_files = ForeignFiles::default();
    configure_materialized_prim(&engine, &mut files);

    let mut registry_files = HashSet::new();
    for package in packages {
        for path in load_folder(&package.join("src"))? {
            let id = load_file(
                &mut engine,
                &mut files,
                &mut foreign_files,
                &path,
                &HashSet::new(),
                &mut BTreeMap::new(),
            )?;
            registry_files.insert(id);
        }
    }

    let replacements_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(folder).join("replacements.json");
    let mut replacements: BTreeMap<String, String> = match fs::read(&replacements_path) {
        Ok(contents) => serde_json::from_slice(&contents)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(error) => return Err(error.into()),
    };
    if replacements.values().any(|reason| reason.trim().is_empty()) {
        return Err(std::io::Error::other("module replacements require a reason").into());
    }
    let mut fixture_files = HashSet::new();
    for path in load_folder(folder)? {
        let id = load_file(
            &mut engine,
            &mut files,
            &mut foreign_files,
            &path,
            &registry_files,
            &mut replacements,
        )?;
        fixture_files.insert(id);
    }
    if !replacements.is_empty() {
        return Err(std::io::Error::other(format!(
            "unused module replacements in {}: {:?}",
            replacements_path.display(),
            replacements.keys(),
        ))
        .into());
    }
    Ok(LoadedFixture { engine, files, fixture_files })
}
