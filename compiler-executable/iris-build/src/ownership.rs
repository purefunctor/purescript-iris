use std::path::{Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};
use smol_str::SmolStr;

use super::packages::{DiscoveredPackage, PackagesError};
use super::walk;

#[derive(Clone, Debug)]
pub(crate) struct SourcePackage {
    pub(crate) name: SmolStr,
    pub(crate) editable: bool,
}

impl SourcePackage {
    pub(crate) fn new(package: &DiscoveredPackage) -> SourcePackage {
        SourcePackage { name: SmolStr::clone(&package.name), editable: package.editable }
    }
}

pub(crate) struct SourceOwnership {
    globs: GlobSet,
    packages: Vec<SourcePackage>,
}

impl SourceOwnership {
    pub(crate) fn new(
        root: &Path,
        packages: &[DiscoveredPackage],
    ) -> Result<SourceOwnership, globset::Error> {
        let mut globs = GlobSetBuilder::new();
        let mut owners = Vec::new();
        for package in packages {
            for glob in &package.source_globs {
                if let Some(glob) = walk::absolute_glob(root, glob) {
                    globs.add(glob);
                    owners.push(SourcePackage::new(package));
                }
            }
        }
        Ok(SourceOwnership { globs: globs.build()?, packages: owners })
    }

    pub(crate) fn owner(&self, path: &Path) -> Result<Option<&SourcePackage>, PackagesError> {
        let mut owners = self.globs.matches(path).into_iter().map(|index| &self.packages[index]);
        let Some(first) = owners.next() else {
            return Ok(None);
        };
        if let Some(second) = owners.find(|owner| owner.name != first.name) {
            return Err(PackagesError::ConflictingSource {
                path: PathBuf::from(path),
                first: SmolStr::clone(&first.name),
                second: SmolStr::clone(&second.name),
            });
        }
        Ok(Some(first))
    }
}
