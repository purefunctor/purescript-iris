//! Naming of published releases. Binaries that are already installed resolve and download releases
//! by these names, so they are a contract with `.github/workflows/release.yml` and `canary.yml`
//! that later releases must keep.

use std::env::consts::EXE_SUFFIX;
use std::fmt;

use serde::Deserialize;
use thiserror::Error;

pub const REPOSITORY: &str = "purefunctor/purescript-iris";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Releases tagged `v<major>.<minor>.<patch>`.
    Stable,
    /// Builds of `main`, tagged `v<version>-dev.<revision>`.
    Canary,
}

/// A published Iris, identified by the version that `iris --version` prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    version: String,
    revision: Option<String>,
}

#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("invalid Iris version `{version}`; expected a version such as v0.1.6")]
    Invalid { version: String },
    #[error("Iris {version} predates the current release layout; use the installer from its tag")]
    Unsupported { version: String },
}

impl Release {
    /// Parses a version with or without the tag's `v` prefix.
    pub fn parse(text: &str) -> Result<Release, ReleaseError> {
        let invalid = || ReleaseError::Invalid { version: text.to_string() };
        let version = text.strip_prefix('v').unwrap_or(text);
        let (base, revision) = match version.split_once("-dev.") {
            Some((base, revision)) => (base, Some(revision)),
            None => (version, None),
        };

        let components = base.split('.').collect::<Vec<_>>();
        let is_number = |component: &&str| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        };
        if components.len() != 3 || !components.iter().all(is_number) {
            return Err(invalid());
        }
        if let Some(revision) = revision {
            let is_revision = (7..=64).contains(&revision.len())
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit());
            if !is_revision {
                return Err(invalid());
            }
        }
        // Releases before v0.1.0 named the executable and archives differently.
        if components[0] == "0" && components[1] == "0" {
            return Err(ReleaseError::Unsupported { version: format!("v{version}") });
        }

        let revision = revision.map(str::to_ascii_lowercase);
        let version = match &revision {
            Some(revision) => format!("{base}-dev.{revision}"),
            None => base.to_string(),
        };
        Ok(Release { version, revision })
    }

    pub fn tag(&self) -> String {
        format!("v{}", self.version)
    }

    pub fn channel(&self) -> Channel {
        if self.revision.is_some() { Channel::Canary } else { Channel::Stable }
    }

    /// What the release's executable prints for `--version`.
    pub fn version_output(&self) -> String {
        format!("iris {}", self.version)
    }

    fn is_built_from(&self, commit: &str) -> bool {
        self.revision.as_deref().is_some_and(|revision| commit.starts_with(revision))
    }
}

impl fmt::Display for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.version)
    }
}

/// The release target whose archive runs on this platform. The universal macOS archive serves
/// both architectures, and Linux builds keep the C library they were built against.
pub fn release_target() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")) {
        Some("x86_64-unknown-linux-gnu")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64", target_env = "musl")) {
        Some("x86_64-unknown-linux-musl")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64", target_env = "gnu")) {
        Some("aarch64-unknown-linux-gnu")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64", target_env = "musl")) {
        Some("aarch64-unknown-linux-musl")
    } else if cfg!(all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64"))) {
        Some("universal-apple-darwin")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")) {
        Some("x86_64-pc-windows-msvc")
    } else {
        None
    }
}

pub fn archive_name(target: &str) -> String {
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    format!("iris-{target}.{extension}")
}

/// The executable's path inside the archive, below the archive's leading directory.
pub fn archive_executable(target: &str) -> String {
    format!("iris-{target}/iris{EXE_SUFFIX}")
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommit {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
}

/// The canary built from the most recent commit on `main` that has one.
///
/// Canaries are published when their checks finish, so publication order does not follow `main`:
/// `v0.1.4-dev.5f263c710632` was published after `v0.1.5`. Several commits pushed together also
/// share one canary, so most commits have none.
pub fn latest_canary(commits: &[GitHubCommit], releases: &[GitHubRelease]) -> Option<Release> {
    let canaries = releases.iter().filter(|release| release.prerelease && !release.draft);
    let canaries = canaries.filter_map(|release| Release::parse(&release.tag_name).ok());
    let canaries = canaries.collect::<Vec<_>>();
    commits.iter().find_map(|commit| {
        canaries.iter().find(|canary| canary.is_built_from(&commit.sha)).cloned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str) -> GitHubCommit {
        GitHubCommit { sha: sha.to_string() }
    }

    fn release(tag_name: &str, prerelease: bool, draft: bool) -> GitHubRelease {
        GitHubRelease { tag_name: tag_name.to_string(), draft, prerelease }
    }

    #[test]
    fn selects_the_canary_of_the_newest_commit_with_a_published_one() {
        let commits = [
            commit("ffffffffffffffffffffffffffffffffffffffff"),
            commit("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"),
            commit("5c8261532bc1aaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            commit("be2f0a13b727aaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ];
        // Listed in publication order, newest first, as the API lists them.
        let releases = [
            release("v0.1.5-dev.eeeeeeeeeeee", true, true),
            release("v0.1.5-dev.be2f0a13b727", true, false),
            release("v0.1.6", false, false),
            release("v0.1.5-dev.5c8261532bc1", true, false),
        ];
        let canary = latest_canary(&commits, &releases).unwrap();
        assert_eq!(canary.tag(), "v0.1.5-dev.5c8261532bc1");
    }
}
