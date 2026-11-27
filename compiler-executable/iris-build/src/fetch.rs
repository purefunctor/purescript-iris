use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::{fs, io};

use iris_spago::MANIFEST_FILE;

use super::workspace::Workspace;

const SPAGO_DIRECTORY: &str = ".spago";
const LOCK_FILE: &str = "spago.lock";
const STAMP_FILE: &str = "iris-fetch-stamp";
const INSTALLED_DIRECTORIES: [&str; 2] = ["p", "g"];

pub(crate) struct FetchStamp {
    path: PathBuf,
    content: String,
}

impl FetchStamp {
    pub(crate) fn of(workspace: &Workspace) -> io::Result<FetchStamp> {
        let spago = workspace.root.join(SPAGO_DIRECTORY);
        let mut lines = vec![format!("selected {}", workspace.selected.as_deref().unwrap_or(""))];
        for manifest in manifests(workspace) {
            lines.push(format!("manifest {} {}", manifest.display(), content_hash(&manifest)?));
        }
        lines.push(format!("lock {}", content_hash(&workspace.root.join(LOCK_FILE))?));
        for directory in INSTALLED_DIRECTORIES {
            let installed = installed(&spago.join(directory))?;
            lines.push(format!("installed {directory} {}", installed.join(" ")));
        }
        Ok(FetchStamp { path: spago.join(STAMP_FILE), content: lines.join("\n") })
    }

    pub(crate) fn is_current(&self) -> bool {
        fs::read_to_string(&self.path).is_ok_and(|recorded| recorded == self.content)
    }

    pub(crate) fn record(&self) -> io::Result<()> {
        if !self.path.parent().is_some_and(Path::is_dir) {
            return Ok(());
        }
        fs::write(&self.path, &self.content)
    }
}

fn manifests(workspace: &Workspace) -> BTreeSet<PathBuf> {
    let packages = workspace.packages.values();
    let package_manifests = packages.map(|package| package.root.join(MANIFEST_FILE));
    let workspace_manifest = workspace.root.join(MANIFEST_FILE);
    package_manifests.chain(std::iter::once(workspace_manifest)).collect()
}

fn content_hash(path: &Path) -> io::Result<String> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok("absent".to_owned()),
        Err(error) => return Err(error),
    };
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn installed(directory: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error),
    };
    let mut names = BTreeSet::new();
    for entry in entries {
        names.insert(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(names.into_iter().collect())
}
