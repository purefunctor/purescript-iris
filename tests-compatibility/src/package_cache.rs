use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use flate2::read::GzDecoder;
use tar::Archive;
use ureq::Agent;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct PackageCache {
    root: PathBuf,
    agent: Agent,
}

impl PackageCache {
    pub fn new(root: PathBuf) -> PackageCache {
        let agent = Agent::config_builder().timeout_global(Some(REQUEST_TIMEOUT)).build().into();
        PackageCache { root, agent }
    }

    pub fn download_path(&self, package: &str, version: &str) -> PathBuf {
        self.root.join("downloads").join(format!("{package}-{version}.tar.gz"))
    }

    pub fn source_path(&self, package: &str, version: &str) -> PathBuf {
        self.root.join("sources").join(package).join(version)
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.root.join("sources")
    }

    pub fn is_package_prepared(&self, package: &str, version: &str) -> bool {
        let download_path = self.download_path(package, version);
        let source_path = self.source_path(package, version);
        let stamp = source_path.join(".tarball-size");

        let Ok(tarball_size) = fs::metadata(download_path).map(|metadata| metadata.len()) else {
            return false;
        };

        let expected_stamp = tarball_size.to_string();
        source_path.exists()
            && fs::read_to_string(stamp).ok().as_deref() == Some(expected_stamp.as_str())
    }

    pub fn ensure_package(&self, package: &str, version: &str) -> Result<PathBuf> {
        let download_path = self.download_path(package, version);
        if !download_path.exists() {
            fs::create_dir_all(download_path.parent().expect("download path has parent"))?;
            let url =
                format!("https://packages.registry.purescript.org/{package}/{version}.tar.gz");
            let response = self.agent.get(&url).call()?;
            // Registry archives are not bounded by ureq's default 10 MB body limit.
            let bytes = response.into_body().into_with_config().limit(u64::MAX).read_to_vec()?;
            fs::write(&download_path, bytes)?;
        }

        let source_path = self.source_path(package, version);
        let stamp = source_path.join(".tarball-size");
        let tarball_size = fs::metadata(&download_path)?.len().to_string();
        let current_stamp = fs::read_to_string(&stamp).ok();

        if current_stamp.as_deref() != Some(tarball_size.as_str()) {
            if source_path.exists() {
                fs::remove_dir_all(&source_path)?;
            }
            fs::create_dir_all(&source_path)?;
            extract_tarball(&download_path, &source_path)?;
            fs::write(stamp, tarball_size)?;
        }

        Ok(source_path)
    }
}

fn extract_tarball(tarball: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(tarball)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(stripped) = stripped_archive_path(&path)? else { continue };
        entry.unpack(destination.join(stripped))?;
    }

    Ok(())
}

fn stripped_archive_path(path: &Path) -> Result<Option<PathBuf>> {
    ensure_relative_archive_path(path)?;
    let stripped = path.components().skip(1).collect::<PathBuf>();
    if stripped.as_os_str().is_empty() {
        return Ok(None);
    }
    ensure_relative_archive_path(&stripped)?;
    Ok(Some(stripped))
}

fn ensure_relative_archive_path(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        bail!("archive entry escapes extraction directory: {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{PackageCache, stripped_archive_path};

    #[test]
    fn deterministic_cache_paths() {
        let cache = PackageCache::new("target/compatibility".into());
        assert_eq!(
            cache.download_path("prelude", "6.0.2").to_string_lossy(),
            "target/compatibility/downloads/prelude-6.0.2.tar.gz"
        );
        assert_eq!(
            cache.source_path("prelude", "6.0.2").to_string_lossy(),
            "target/compatibility/sources/prelude/6.0.2"
        );
    }

    #[test]
    fn rejects_absolute_archive_paths() {
        let error = stripped_archive_path(Path::new("/package/src/Main.purs")).unwrap_err();
        assert!(error.to_string().contains("archive entry escapes extraction directory"));
    }

    #[test]
    fn rejects_parent_archive_paths() {
        let error = stripped_archive_path(Path::new("package/../Main.purs")).unwrap_err();
        assert!(error.to_string().contains("archive entry escapes extraction directory"));
    }

    #[test]
    fn strips_archive_package_directory() {
        let path = stripped_archive_path(Path::new("package/src/Main.purs")).unwrap().unwrap();
        assert_eq!(path, Path::new("src/Main.purs"));
    }
}
