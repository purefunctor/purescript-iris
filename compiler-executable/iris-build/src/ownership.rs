use std::path::{Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};
use smol_str::SmolStr;

use super::packages::{DiscoveredPackage, PackagesError};
use super::walk;

pub(crate) struct SourceOwnership {
    globs: GlobSet,
    packages: Vec<SmolStr>,
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
                    owners.push(SmolStr::clone(&package.name));
                }
            }
        }
        Ok(SourceOwnership { globs: globs.build()?, packages: owners })
    }

    pub(crate) fn claims(&self, path: &Path) -> bool {
        self.globs.is_match(path)
    }

    pub(crate) fn owner(&self, path: &Path) -> Result<Option<&SmolStr>, PackagesError> {
        let mut owners = self.globs.matches(path).into_iter().map(|index| &self.packages[index]);
        let Some(first) = owners.next() else {
            return Ok(None);
        };
        if let Some(second) = owners.find(|owner| *owner != first) {
            return Err(PackagesError::ConflictingSource {
                path: PathBuf::from(path),
                first: SmolStr::clone(first),
                second: SmolStr::clone(second),
            });
        }
        Ok(Some(first))
    }
}
