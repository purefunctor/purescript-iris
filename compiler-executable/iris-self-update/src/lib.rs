//! `iris self update`: replaces the running Iris with an archive published by the release and
//! canary workflows, trusting what `install.sh` and `install.ps1` trust.
//!
//! Iris is a single executable, so the update replaces whichever file is running, however it was
//! installed. A package manager that installed it sees the new file on its next install.

mod archive;
mod attestation;
mod github;
mod install;
mod release;

use std::fs::File;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::archive::ArchiveError;
use crate::attestation::AttestationError;
use crate::github::{GitHub, GitHubError};
use crate::install::{InstallError, Staged};

pub use crate::release::{Channel, Release, ReleaseError};

pub struct SelfUpdateConfig {
    /// The running Iris's version, whose channel is followed unless `channel` says otherwise.
    pub current: String,
    /// A release to install instead of the latest one.
    pub version: Option<String>,
    pub channel: Option<Channel>,
}

pub enum Availability {
    UpToDate(Release),
    Available(Release),
}

pub enum Update {
    AlreadyInstalled(Release),
    Installed(Installed),
}

pub struct Installed {
    pub release: Release,
    pub executable: PathBuf,
    /// Whether the installed release can update itself, which releases before
    /// `iris self update` cannot.
    pub has_self_update: bool,
}

#[derive(Debug, Error)]
pub enum SelfUpdateError {
    #[error("no Iris release is built for this platform; update Iris with the tool that built it")]
    UnsupportedPlatform,
    #[error(transparent)]
    Release {
        #[from]
        source: ReleaseError,
    },
    #[error(transparent)]
    GitHub {
        #[from]
        source: GitHubError,
    },
    #[error(transparent)]
    Attestation {
        #[from]
        source: AttestationError,
    },
    #[error("invalid release archive {archive}: {source}")]
    Archive {
        archive: String,
        #[source]
        source: ArchiveError,
    },
    #[error(transparent)]
    Install {
        #[from]
        source: InstallError,
    },
    #[error("failed to create a temporary download directory: {source}")]
    TemporaryDirectory {
        #[source]
        source: io::Error,
    },
}

/// Reports whether `config` selects a release other than the running one.
pub fn check(config: SelfUpdateConfig) -> Result<Availability, SelfUpdateError> {
    // A release this platform cannot install is not available to it.
    release::release_target().ok_or(SelfUpdateError::UnsupportedPlatform)?;
    let current = Release::parse(&config.current)?;
    let github = GitHub::new(&user_agent(&current));
    let release = resolve(&github, &config, &current)?;
    if release == current {
        Ok(Availability::UpToDate(release))
    } else {
        Ok(Availability::Available(release))
    }
}

pub fn update(config: SelfUpdateConfig) -> Result<Update, SelfUpdateError> {
    let target = release::release_target().ok_or(SelfUpdateError::UnsupportedPlatform)?;
    let current = Release::parse(&config.current)?;
    let github = GitHub::new(&user_agent(&current));
    let release = resolve(&github, &config, &current)?;
    if release == current {
        return Ok(Update::AlreadyInstalled(release));
    }
    let executable = install::current_executable()?;
    install::check_writable(&executable)?;

    let temporary_directory_error = |source| SelfUpdateError::TemporaryDirectory { source };
    let download_directory = tempfile::tempdir().map_err(temporary_directory_error)?;
    let archive = release::archive_name(target);
    let archive_path = download_directory.path().join(&archive);
    eprintln!("Downloading iris {} for {target}", release.tag());
    github.download(&release, &archive, &archive_path)?;
    attestation::verify(&archive_path)?;

    let archive_error = |source| SelfUpdateError::Archive { archive: archive.clone(), source };
    let archive_file = File::open(&archive_path).map_err(|error| archive_error(error.into()))?;
    let mut contents = Vec::new();
    archive::extract_executable(archive_file, &release::archive_executable(target), &mut contents)
        .map_err(archive_error)?;

    let staged = Staged::create(&executable, &contents)?;
    staged.check_version(&release)?;
    let has_self_update = staged.has_self_update();
    staged.install(&executable)?;
    Ok(Update::Installed(Installed { release, executable, has_self_update }))
}

fn resolve(
    github: &GitHub,
    config: &SelfUpdateConfig,
    current: &Release,
) -> Result<Release, SelfUpdateError> {
    if let Some(version) = &config.version {
        return Ok(Release::parse(version)?);
    }
    let release = match config.channel.unwrap_or(current.channel()) {
        Channel::Stable => github.latest_stable()?,
        Channel::Canary => github.latest_canary()?,
    };
    Ok(release)
}

fn user_agent(current: &Release) -> String {
    format!("iris/{current}")
}
