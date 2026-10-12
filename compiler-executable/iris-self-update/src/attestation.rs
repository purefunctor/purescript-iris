//! Release provenance, verified with the same policy as `install.sh` and `install.ps1`: GitHub
//! CLI's attestation verification when it is installed, a warning when it is not, and
//! `IRIS_SKIP_ATTESTATION=1` to skip it.

use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, io};

use thiserror::Error;

use crate::release::REPOSITORY;

/// The status `gh` exits with when it is not signed in.
const GITHUB_CLI_SIGNED_OUT: i32 = 4;

#[derive(Debug, Error)]
pub enum AttestationError {
    #[error("the GitHub CLI is not signed in; run `gh auth login`, or set IRIS_SKIP_ATTESTATION=1")]
    SignedOut,
    #[error("GitHub release attestation verification failed")]
    Failed,
    #[error("failed to run the GitHub CLI: {source}")]
    Run {
        #[source]
        source: io::Error,
    },
}

pub fn verify(archive: &Path) -> Result<(), AttestationError> {
    if env::var_os("IRIS_SKIP_ATTESTATION").is_some_and(|value| value == "1") {
        eprintln!(
            "warning: GitHub release attestation verification skipped (IRIS_SKIP_ATTESTATION=1)."
        );
        return Ok(());
    }

    let help = Command::new("gh")
        .args(["attestation", "verify", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !help.is_ok_and(|status| status.success()) {
        eprintln!(
            "warning: A GitHub CLI with attestation support is not installed; release provenance was not verified."
        );
        eprintln!(
            "warning: Install or update gh from https://cli.github.com/ to verify future updates."
        );
        return Ok(());
    }

    eprintln!("Verifying GitHub release attestation");
    let status = Command::new("gh")
        .args(["attestation", "verify"])
        .arg(archive)
        .args(["--repo", REPOSITORY])
        .stdout(Stdio::null())
        .status()
        .map_err(|source| AttestationError::Run { source })?;
    match status.code() {
        Some(0) => Ok(()),
        Some(GITHUB_CLI_SIGNED_OUT) => Err(AttestationError::SignedOut),
        _ => Err(AttestationError::Failed),
    }
}
