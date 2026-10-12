use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use flate2::read::GzDecoder;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, EntryType};
use ureq::Agent;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const COMPLETE_FILE: &str = ".complete";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PackageConfiguration {
    package_set: String,
    roots: Vec<String>,
}

fn read_configuration(path: &Path) -> Result<(PackageConfiguration, String)> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut configuration: PackageConfiguration = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Version::parse(&configuration.package_set).context("invalid package set version")?;
    ensure!(!configuration.roots.is_empty(), "package configuration has no roots");
    for root in &configuration.roots {
        validate_package_name(root)?;
    }
    configuration.roots.sort();
    configuration.roots.dedup();
    let key = digest_hex(&serde_json::to_vec(&configuration)?);
    Ok((configuration, key))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RegistryLock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_set: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<String>,
    pub packages: Vec<LockedPackage>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LockedPackage {
    pub name: String,
    pub version: String,
    pub sha256: String,
}

fn read_lock(path: impl AsRef<Path>) -> Result<RegistryLock> {
    let path = path.as_ref();
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lock: RegistryLock = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_lock(&lock)?;
    Ok(lock)
}

fn validate_lock(lock: &RegistryLock) -> Result<()> {
    ensure!(!lock.packages.is_empty(), "registry lock has no packages");
    if let Some(package_set) = &lock.package_set {
        Version::parse(package_set).context("invalid package set version")?;
    }
    let mut names = std::collections::BTreeSet::new();
    for package in &lock.packages {
        validate_package_name(&package.name)?;
        Version::parse(&package.version)
            .with_context(|| format!("invalid package version: {:?}", package.version))?;
        ensure!(
            package.sha256.len() == 64
                && package
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "invalid SHA256 digest for {}@{}",
            package.name,
            package.version
        );
        ensure!(names.insert(&package.name), "duplicate package name {}", package.name);
    }
    for root in &lock.roots {
        validate_package_name(root)?;
        ensure!(names.contains(root), "root package {root} is not locked");
    }
    Ok(())
}

#[derive(Deserialize)]
struct PackageSet {
    packages: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Manifest {
    name: String,
    version: String,
    dependencies: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Metadata {
    published: std::collections::BTreeMap<String, Published>,
}

#[derive(Deserialize)]
struct Published {
    hash: String,
}

fn resolve_packages(configuration: &PackageConfiguration) -> Result<RegistryLock> {
    let package_set_version = &configuration.package_set;
    let agent = http_agent();
    let base = "https://raw.githubusercontent.com/purescript";
    let package_set_url = format!("{base}/registry/main/package-sets/{package_set_version}.json");
    let package_set_response = agent.get(&package_set_url).call()?.body_mut().read_to_string()?;
    let package_set: PackageSet = serde_json::from_str(&package_set_response)?;
    build_lock(package_set_version, configuration.roots.clone(), package_set, |path| {
        Ok(agent.get(&format!("{base}/{path}")).call()?.body_mut().read_to_string()?)
    })
}

fn build_lock<F>(
    package_set_version: &str,
    mut roots: Vec<String>,
    package_set: PackageSet,
    mut fetch: F,
) -> Result<RegistryLock>
where
    F: FnMut(&str) -> Result<String>,
{
    roots.sort();
    roots.dedup();
    let mut pending = roots.clone();
    let mut packages = std::collections::BTreeMap::new();
    while let Some(name) = pending.pop() {
        validate_package_name(&name)?;
        if packages.contains_key(&name) {
            continue;
        }
        let version = package_set
            .packages
            .get(&name)
            .with_context(|| format!("package set does not contain {name}"))?;
        Version::parse(version).with_context(|| format!("invalid selected version for {name}"))?;
        let index = fetch(&format!("registry-index/main/{}", registry_index_path(&name)))?;
        let manifest = index
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<Manifest>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .find(|manifest| manifest.version == *version)
            .with_context(|| format!("registry index has no manifest for {name}@{version}"))?;
        ensure!(manifest.name == name, "registry manifest name mismatch for {name}@{version}");
        for (dependency, requirement) in manifest.dependencies {
            let selected = package_set.packages.get(&dependency).with_context(|| {
                format!("package set does not contain dependency {dependency} of {name}")
            })?;
            let requirement = requirement.split_whitespace().collect::<Vec<_>>().join(", ");
            let requirement = VersionReq::parse(&requirement)
                .with_context(|| format!("invalid dependency range for {dependency} in {name}"))?;
            let selected_version = Version::parse(selected)?;
            ensure!(
                requirement.matches(&selected_version),
                "selected {dependency}@{selected} does not satisfy {requirement} required by {name}"
            );
            pending.push(dependency);
        }
        let metadata: Metadata =
            serde_json::from_str(&fetch(&format!("registry/main/metadata/{name}.json"))?)?;
        let hash = &metadata
            .published
            .get(version)
            .with_context(|| format!("registry metadata has no hash for {name}@{version}"))?
            .hash;
        let encoded = hash
            .strip_prefix("sha256-")
            .with_context(|| format!("invalid registry hash for {name}@{version}"))?;
        let digest = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .with_context(|| format!("invalid registry hash for {name}@{version}"))?;
        ensure!(digest.len() == 32, "invalid registry hash for {name}@{version}");
        packages.insert(
            name.clone(),
            LockedPackage { name, version: version.clone(), sha256: hex(&digest) },
        );
    }
    let lock = RegistryLock {
        package_set: Some(package_set_version.into()),
        roots,
        packages: packages.into_values().collect(),
    };
    validate_lock(&lock)?;
    Ok(lock)
}

fn registry_index_path(name: &str) -> String {
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

/// Resolves a pinned package set and prepares its sources. Resolution is cached locally.
pub fn prepare(
    configuration_path: impl AsRef<Path>,
    cache_root: impl AsRef<Path>,
) -> Result<Vec<PathBuf>> {
    let cache_root = cache_root.as_ref();
    let (configuration, configuration_key) = read_configuration(configuration_path.as_ref())?;
    fs::create_dir_all(cache_root)?;
    let lock_file = File::create(cache_root.join(".prepare.lock"))?;
    lock_file.lock()?;
    let resolution_path = cache_root.join("resolutions").join(format!("{configuration_key}.json"));
    let lock = if resolution_path.exists() {
        read_resolution(&configuration, &resolution_path)?
    } else {
        let lock = resolve_packages(&configuration)?;
        fs::create_dir_all(cache_root.join("resolutions"))?;
        atomic_write(&resolution_path, &serde_json::to_vec_pretty(&lock)?)?;
        lock
    };
    let key = digest_hex(&serde_json::to_vec(&lock)?);
    let agent = http_agent();
    prepare_locked(&lock, &key, cache_root, |package| {
        let url = format!(
            "https://packages.registry.purescript.org/{}/{}.tar.gz",
            package.name, package.version
        );
        let response = agent.get(&url).call()?;
        // Registry archives are not bounded by ureq's default 10 MB body limit.
        Ok(response.into_body().into_with_config().limit(u64::MAX).read_to_vec()?)
    })?;
    prepared_sources_for_lock(&lock, &key, cache_root)
}

fn http_agent() -> Agent {
    Agent::config_builder().timeout_global(Some(REQUEST_TIMEOUT)).build().into()
}

fn read_resolution(configuration: &PackageConfiguration, path: &Path) -> Result<RegistryLock> {
    let lock = read_lock(path)?;
    ensure!(
        lock.package_set.as_ref() == Some(&configuration.package_set)
            && lock.roots == configuration.roots,
        "cached resolution does not match package configuration"
    );
    Ok(lock)
}

/// Returns package roots only when the configuration's source set is complete.
/// This function performs no network access.
pub fn prepared_sources(
    configuration_path: impl AsRef<Path>,
    cache_root: impl AsRef<Path>,
) -> Result<Vec<PathBuf>> {
    let cache_root = cache_root.as_ref();
    let (configuration, key) = read_configuration(configuration_path.as_ref())?;
    let resolution_path = cache_root.join("resolutions").join(format!("{key}.json"));
    let lock = read_resolution(&configuration, &resolution_path)?;
    let key = digest_hex(&serde_json::to_vec(&lock)?);
    prepared_sources_for_lock(&lock, &key, cache_root)
}

fn prepare_locked<F>(lock: &RegistryLock, key: &str, cache_root: &Path, mut fetch: F) -> Result<()>
where
    F: FnMut(&LockedPackage) -> Result<Vec<u8>>,
{
    let destination = cache_root.join("source-sets").join(key);
    if source_set_complete(lock, key, &destination) {
        return Ok(());
    }
    ensure!(
        !destination.exists(),
        "incomplete immutable source set exists at {}",
        destination.display()
    );

    fs::create_dir_all(cache_root.join("source-sets"))?;
    fs::create_dir_all(cache_root.join("downloads"))?;
    let temporary = cache_root.join("source-sets").join(format!(".{key}.tmp-{}", unique_suffix()));
    fs::create_dir(&temporary)?;
    let result = (|| {
        for package in &lock.packages {
            let download = cache_root.join("downloads").join(format!("{}.tar.gz", package.sha256));
            let bytes = match fs::read(&download) {
                Ok(bytes) if digest_hex(&bytes) == package.sha256 => bytes,
                _ => {
                    let bytes = fetch(package)?;
                    verify_digest(package, &bytes)?;
                    atomic_write(&download, &bytes)?;
                    bytes
                }
            };
            verify_digest(package, &bytes)?;
            let package_root = temporary.join(&package.name).join(&package.version);
            fs::create_dir_all(&package_root)?;
            extract_tarball(&bytes, &package_root)?;
            ensure!(
                package_root.join("src").is_dir(),
                "{}@{} archive has no src directory",
                package.name,
                package.version
            );
        }
        fs::write(temporary.join(COMPLETE_FILE), key)?;
        fs::rename(&temporary, &destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn prepared_sources_for_lock(
    lock: &RegistryLock,
    key: &str,
    cache_root: &Path,
) -> Result<Vec<PathBuf>> {
    let destination = cache_root.join("source-sets").join(key);
    ensure!(
        source_set_complete(lock, key, &destination),
        "registry sources are not prepared for lock {key}"
    );
    Ok(lock
        .packages
        .iter()
        .map(|package| destination.join(&package.name).join(&package.version))
        .collect())
}

fn source_set_complete(lock: &RegistryLock, key: &str, destination: &Path) -> bool {
    fs::read_to_string(destination.join(COMPLETE_FILE)).ok().as_deref() == Some(key)
        && lock.packages.iter().all(|package| {
            destination.join(&package.name).join(&package.version).join("src").is_dir()
        })
}

fn verify_digest(package: &LockedPackage, bytes: &[u8]) -> Result<()> {
    ensure!(
        digest_hex(bytes) == package.sha256,
        "SHA256 mismatch for {}@{}",
        package.name,
        package.version
    );
    Ok(())
}

fn extract_tarball(bytes: &[u8], destination: &Path) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        ensure!(
            entry_type == EntryType::Regular || entry_type == EntryType::Directory,
            "archive contains link or special file"
        );
        let path = entry.path()?.into_owned();
        let Some(path) = stripped_archive_path(&path)? else { continue };
        let output = destination.join(path);
        if entry_type == EntryType::Directory {
            fs::create_dir_all(output)?;
        } else {
            fs::create_dir_all(output.parent().expect("archive output has parent"))?;
            let mut file = File::create(output)?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    Ok(())
}

fn stripped_archive_path(path: &Path) -> Result<Option<PathBuf>> {
    ensure_relative_path(path)?;
    let stripped = path.components().skip(1).collect::<PathBuf>();
    if stripped.as_os_str().is_empty() {
        return Ok(None);
    }
    ensure_relative_path(&stripped)?;
    Ok(Some(stripped))
}

fn ensure_relative_path(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("archive entry escapes extraction directory: {}", path.display());
    }
    Ok(())
}

fn validate_package_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !value.starts_with('-')
            && !value.ends_with('-'),
        "invalid package name: {value:?}"
    );
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [DIGITS[(byte >> 4) as usize] as char, DIGITS[(byte & 15) as usize] as char]
        })
        .collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", unique_suffix()));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tempfile::tempdir;

    fn archive(entry_type: EntryType, path: &str, contents: &[u8]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(entry_type);
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, contents).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn fixture_lock(bytes: &[u8]) -> RegistryLock {
        RegistryLock {
            package_set: None,
            roots: vec!["prelude".into()],
            packages: vec![LockedPackage {
                name: "prelude".into(),
                version: "6.0.2".into(),
                sha256: digest_hex(bytes),
            }],
        }
    }

    fn package_set(packages: &[(&str, &str)]) -> PackageSet {
        PackageSet {
            packages: packages
                .iter()
                .map(|(name, version)| ((*name).into(), (*version).into()))
                .collect(),
        }
    }

    type ManifestFixture<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);

    fn registry_fetch<'a>(
        manifests: &'a [ManifestFixture<'a>],
        hashes: &'a [(&str, &str)],
    ) -> impl FnMut(&str) -> Result<String> + 'a {
        move |path| {
            if path.contains("registry-index") {
                let name = path.rsplit('/').next().unwrap();
                let (_, version, dependencies) = manifests
                    .iter()
                    .find(|item| item.0 == name)
                    .with_context(|| format!("missing manifest fixture for {name}"))?;
                Ok(serde_json::json!({ "name": name, "version": version,
                    "dependencies": dependencies.iter().copied().collect::<std::collections::BTreeMap<_, _>>() }).to_string())
            } else {
                let name = path.strip_suffix(".json").unwrap().rsplit('/').next().unwrap();
                let (_, hash) = hashes
                    .iter()
                    .find(|item| item.0 == name)
                    .with_context(|| format!("missing metadata fixture for {name}"))?;
                Ok(serde_json::json!({ "published": { "1.0.0": { "hash": hash } } }).to_string())
            }
        }
    }

    #[test]
    fn builds_complete_sorted_lock() {
        let hash = format!("sha256-{}", STANDARD.encode([7; 32]));
        let lock = build_lock(
            "1.0.0",
            vec!["root".into()],
            package_set(&[("root", "1.0.0"), ("dependency", "1.0.0")]),
            registry_fetch(
                &[
                    ("root", "1.0.0", &[("dependency", ">=1.0.0 <2.0.0")]),
                    ("dependency", "1.0.0", &[]),
                ],
                &[("root", &hash), ("dependency", &hash)],
            ),
        )
        .unwrap();
        assert_eq!(
            lock.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(),
            ["dependency", "root"]
        );
    }

    #[test]
    fn rejects_incomplete_closure_and_range_mismatch() {
        let hash = format!("sha256-{}", STANDARD.encode([7; 32]));
        let manifests = [("root", "1.0.0", [("dependency", ">=2.0.0 <3.0.0")].as_slice())];
        let hashes = [("root", hash.as_str())];
        let incomplete = build_lock(
            "1.0.0",
            vec!["root".into()],
            package_set(&[("root", "1.0.0")]),
            registry_fetch(&manifests, &hashes),
        )
        .unwrap_err();
        assert!(incomplete.to_string().contains("does not contain dependency"));
        let mismatch = build_lock(
            "1.0.0",
            vec!["root".into()],
            package_set(&[("root", "1.0.0"), ("dependency", "1.0.0")]),
            registry_fetch(&manifests, &hashes),
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("does not satisfy"));
    }

    #[test]
    fn rejects_invalid_registry_hash() {
        let error = build_lock(
            "1.0.0",
            vec!["root".into()],
            package_set(&[("root", "1.0.0")]),
            registry_fetch(&[("root", "1.0.0", &[])], &[("root", "sha256-not-base64")]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid registry hash"));
    }

    #[test]
    fn validates_names_versions_roots_and_unique_names() {
        let mut lock = fixture_lock(b"bytes");
        lock.roots = vec!["missing".into()];
        assert!(validate_lock(&lock).unwrap_err().to_string().contains("not locked"));
        lock.roots = vec!["prelude".into()];
        lock.packages.push(LockedPackage {
            name: "prelude".into(),
            version: "7.0.0".into(),
            sha256: "0".repeat(64),
        });
        assert!(validate_lock(&lock).unwrap_err().to_string().contains("duplicate package name"));
        lock.packages.pop();
        lock.packages[0].name = "../Prelude".into();
        assert!(validate_lock(&lock).unwrap_err().to_string().contains("invalid package name"));
        lock.packages[0].name = "prelude".into();
        lock.packages[0].version = "not-a-version".into();
        assert!(validate_lock(&lock).unwrap_err().to_string().contains("invalid package version"));
    }

    #[test]
    fn rejects_integrity_mismatch() {
        let package = &fixture_lock(b"expected").packages[0];
        assert!(
            verify_digest(package, b"different")
                .unwrap_err()
                .to_string()
                .contains("SHA256 mismatch")
        );
    }

    #[test]
    fn rejects_archive_links_and_traversal() {
        let directory = tempdir().unwrap();
        let link = archive(EntryType::Symlink, "package/link", b"target");
        assert!(
            extract_tarball(&link, directory.path())
                .unwrap_err()
                .to_string()
                .contains("special file")
        );
        assert!(stripped_archive_path(Path::new("package/../outside")).is_err());
    }

    #[test]
    fn incomplete_cache_is_not_readable() {
        let bytes = archive(EntryType::Regular, "package/src/Main.purs", b"module Main where");
        let lock = fixture_lock(&bytes);
        let lock_bytes = serde_json::to_vec(&lock).unwrap();
        let key = digest_hex(&lock_bytes);
        let directory = tempdir().unwrap();
        assert!(prepared_sources_for_lock(&lock, &key, directory.path()).is_err());
        prepare_locked(&lock, &key, directory.path(), |_| Ok(bytes.clone())).unwrap();
        let roots = prepared_sources_for_lock(&lock, &key, directory.path()).unwrap();
        assert_eq!(
            fs::read_to_string(roots[0].join("src/Main.purs")).unwrap(),
            "module Main where"
        );
    }

    #[test]
    fn warm_cache_does_not_fetch() {
        let bytes = archive(EntryType::Regular, "package/src/Main.purs", b"module Main where");
        let lock = fixture_lock(&bytes);
        let key = digest_hex(&serde_json::to_vec(&lock).unwrap());
        let directory = tempdir().unwrap();
        prepare_locked(&lock, &key, directory.path(), |_| Ok(bytes.clone())).unwrap();
        prepare_locked(&lock, &key, directory.path(), |_| bail!("network unavailable")).unwrap();
    }

    #[test]
    fn concurrent_preparations_publish_one_source_set() {
        let bytes = archive(EntryType::Regular, "package/src/Main.purs", b"module Main where");
        let mut lock = fixture_lock(&bytes);
        lock.package_set = Some("1.0.0".into());
        let directory = tempdir().unwrap();
        let configuration_path = directory.path().join("packages.json");
        let cache_root = directory.path().join("cache");
        fs::write(&configuration_path, r#"{"package_set":"1.0.0","roots":["prelude"]}"#).unwrap();
        let (_, key) = read_configuration(&configuration_path).unwrap();
        fs::create_dir_all(cache_root.join("downloads")).unwrap();
        fs::create_dir_all(cache_root.join("resolutions")).unwrap();
        fs::write(
            cache_root.join("resolutions").join(format!("{key}.json")),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        fs::write(
            cache_root.join("downloads").join(format!("{}.tar.gz", lock.packages[0].sha256)),
            bytes,
        )
        .unwrap();

        std::thread::scope(|scope| {
            let first = scope.spawn(|| prepare(&configuration_path, &cache_root));
            let second = scope.spawn(|| prepare(&configuration_path, &cache_root));
            assert_eq!(first.join().unwrap().unwrap(), second.join().unwrap().unwrap());
        });
        assert_eq!(prepared_sources(&configuration_path, &cache_root).unwrap().len(), 1);
        fs::write(&configuration_path, r#"{"package_set":"2.0.0","roots":["prelude"]}"#).unwrap();
        assert!(prepared_sources(&configuration_path, &cache_root).is_err());
    }

    #[test]
    fn configuration_key_tracks_versions_and_roots_not_formatting() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("packages.json");
        fs::write(&path, r#"{"package_set":"1.0.0","roots":["effect","prelude"]}"#).unwrap();
        let (_, original) = read_configuration(&path).unwrap();
        fs::write(&path, r#"{ "roots": ["prelude", "effect"], "package_set": "1.0.0" }"#).unwrap();
        assert_eq!(read_configuration(&path).unwrap().1, original);
        fs::write(&path, r#"{"package_set":"2.0.0","roots":["effect","prelude"]}"#).unwrap();
        assert_ne!(read_configuration(&path).unwrap().1, original);
        fs::write(&path, r#"{"package_set":"1.0.0","roots":["prelude"]}"#).unwrap();
        assert_ne!(read_configuration(&path).unwrap().1, original);
        fs::write(&path, r#"{"package_set":"latest","roots":["prelude"]}"#).unwrap();
        assert!(read_configuration(&path).is_err());
        fs::write(&path, r#"{"package_set":"1.0.0","roots":[]}"#).unwrap();
        assert!(read_configuration(&path).is_err());
    }

    #[test]
    fn rejects_resolution_for_different_configuration() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("resolution.json");
        let mut lock = fixture_lock(b"bytes");
        lock.package_set = Some("1.0.0".into());
        fs::write(&path, serde_json::to_vec(&lock).unwrap()).unwrap();
        let mut configuration =
            PackageConfiguration { package_set: "1.0.0".into(), roots: vec!["prelude".into()] };
        assert!(read_resolution(&configuration, &path).is_ok());
        configuration.package_set = "2.0.0".into();
        assert!(read_resolution(&configuration, &path).is_err());
        configuration.package_set = "1.0.0".into();
        configuration.roots = vec!["effect".into()];
        assert!(read_resolution(&configuration, &path).is_err());
    }
}
