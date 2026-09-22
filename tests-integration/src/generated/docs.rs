use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::Path;

use building::QueryEngine;
use files::{FileId, Files};
use itertools::Itertools;
use serde_json::json;
use url::Url;

const PACKAGE_NAME: &str = "docs-fixture";
const PACKAGE_VERSION: &str = "0.0.0";

pub fn report(engine: &QueryEngine, files: &Files, root: &Path) -> Result<String, Box<dyn Error>> {
    let root = Url::from_directory_path(root)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid fixture root"))?
        .to_string();

    let mut modules = files
        .iter_id()
        .filter(|&id| files.path(id).starts_with(&root))
        .filter(|&id| module_name(engine, id).is_some())
        .collect_vec();
    modules.sort_by_key(|&id| module_name(engine, id).unwrap_or_default());

    let dependencies = BTreeMap::new();
    let package = iris_documentation::PackageInput {
        name: PACKAGE_NAME,
        version: PACKAGE_VERSION,
        license: None,
        description: None,
        dependencies: &dependencies,
        location: None,
        modules: &modules,
    };

    let manifest = iris_documentation::render_package_manifest(engine, &package)?;
    let package_by_file = modules.iter().map(|&id| (id, PACKAGE_NAME)).collect_vec();

    let modules = modules
        .into_iter()
        .map(|id| iris_documentation::render_module(engine, id, &package_by_file));
    let modules = modules.collect::<Result<Vec<_>, _>>()?;
    let modules = modules.into_iter().flatten();
    let mut modules = modules.collect_vec();
    modules.sort_by(|left, right| left.name.cmp(&right.name));

    let report = json!({
        "manifest": manifest,
        "modules": modules,
    });

    Ok(serde_json::to_string_pretty(&report)?)
}

fn module_name(engine: &QueryEngine, id: FileId) -> Option<String> {
    let content = engine.content(id).ok()?;
    let (parsed, _) = engine.parsed(id).ok()?;
    parsed.module_name(&content).map(|name| name.to_string())
}
