//! Resolving and downloading releases from GitHub.

use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

use thiserror::Error;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use crate::release::{self, GitHubCommit, GitHubRelease, REPOSITORY, Release, ReleaseError};

const DOWNLOAD_ATTEMPTS: u32 = 3;

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("failed to reach {url}: {source}")]
    Request {
        url: String,
        #[source]
        source: Box<ureq::Error>,
    },
    #[error("failed to download {url}: {source}")]
    Transfer {
        url: String,
        #[source]
        source: io::Error,
    },
    #[error("GitHub sent an unexpected response from {url}: {source}")]
    Response {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("GitHub refused {url}, usually because of its rate limit; set GH_TOKEN to raise it")]
    RateLimited { url: String },
    #[error("GitHub has no stable Iris release")]
    NoStableRelease,
    #[error("none of the recent commits on main has a published canary")]
    NoCanaryRelease,
    #[error("GitHub reported an invalid latest release: {source}")]
    InvalidLatestRelease {
        #[source]
        source: ReleaseError,
    },
    #[error("Iris {release} has no release archive named {archive}")]
    MissingArchive { release: Release, archive: String },
    #[error("failed to write {}: {source}", .path.display())]
    Write {
        path: Box<Path>,
        #[source]
        source: io::Error,
    },
}

pub struct GitHub {
    agent: Agent,
    /// Raises the API's rate limit, which runners sharing an address quickly exhaust.
    token: Option<String>,
}

impl GitHub {
    pub fn new(user_agent: &str) -> GitHub {
        // Use the platform's certificates, as the installers' curl and Invoke-WebRequest do, so
        // that proxies with private certificate authorities keep working.
        let tls = TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build();
        let config = Agent::config_builder()
            .tls_config(tls)
            .https_only(true)
            .user_agent(user_agent)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            // ureq has no idle timeout, so a stalled body is bounded by a total that a slow
            // connection still downloads an archive within, after which the download is retried.
            .timeout_recv_body(Some(Duration::from_secs(600)))
            .build();
        let token = ["GH_TOKEN", "GITHUB_TOKEN"]
            .into_iter()
            .find_map(|name| env::var(name).ok().filter(|token| !token.is_empty()));
        GitHub { agent: config.into(), token }
    }

    /// Reads the latest stable release from the redirect that `install.sh` also follows, which,
    /// unlike the API, is not rate limited.
    pub fn latest_stable(&self) -> Result<Release, GitHubError> {
        let url = format!("https://github.com/{REPOSITORY}/releases/latest");
        let request = self.agent.get(&url).config().max_redirects(0).build();
        let response = request.call().map_err(|source| request_error(&url, source))?;
        let location = response.headers().get("location").and_then(|value| value.to_str().ok());
        let tag = location.and_then(|location| location.rsplit_once("/releases/tag/"));
        let (_, tag) = tag.ok_or(GitHubError::NoStableRelease)?;
        Release::parse(tag).map_err(|source| GitHubError::InvalidLatestRelease { source })
    }

    pub fn latest_canary(&self) -> Result<Release, GitHubError> {
        let commits: Vec<GitHubCommit> = self.api("commits?sha=main&per_page=100")?;
        let releases: Vec<GitHubRelease> = self.api("releases?per_page=100")?;
        release::latest_canary(&commits, &releases).ok_or(GitHubError::NoCanaryRelease)
    }

    pub fn download(
        &self,
        release: &Release,
        archive: &str,
        path: &Path,
    ) -> Result<(), GitHubError> {
        let url = format!(
            "https://github.com/{REPOSITORY}/releases/download/{}/{archive}",
            release.tag()
        );
        let mut attempt = 1;
        loop {
            match self.download_once(&url, path) {
                Err(GitHubError::Request { source, .. })
                    if matches!(*source, ureq::Error::StatusCode(404)) =>
                {
                    let release = release.clone();
                    return Err(GitHubError::MissingArchive {
                        release,
                        archive: archive.to_string(),
                    });
                }
                Err(GitHubError::Request { .. } | GitHubError::Transfer { .. })
                    if attempt < DOWNLOAD_ATTEMPTS =>
                {
                    attempt += 1
                }
                result => return result,
            }
        }
    }

    fn download_once(&self, url: &str, path: &Path) -> Result<(), GitHubError> {
        let write_error = |source| GitHubError::Write { path: path.into(), source };
        let request = self.agent.get(url);
        let mut response = request.call().map_err(|source| request_error(url, source))?;
        let mut file = File::create(path).map_err(write_error)?;
        let mut body = response.body_mut().as_reader();
        // Copying by hand keeps a dropped connection, which is worth retrying, apart from a
        // failed write, which is not.
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let length = body
                .read(&mut buffer)
                .map_err(|source| GitHubError::Transfer { url: url.to_string(), source })?;
            if length == 0 {
                return Ok(());
            }
            file.write_all(&buffer[..length]).map_err(write_error)?;
        }
    }

    fn api<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, GitHubError> {
        let url = format!("https://api.github.com/repos/{REPOSITORY}/{path}");
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let response = request.call().map_err(|source| match source {
            ureq::Error::StatusCode(403 | 429) => GitHubError::RateLimited { url: url.clone() },
            source => request_error(&url, source),
        });
        let body = response?.body_mut().read_to_string();
        let body = body.map_err(|source| request_error(&url, source))?;
        serde_json::from_str(&body).map_err(|source| GitHubError::Response { url, source })
    }
}

fn request_error(url: &str, source: ureq::Error) -> GitHubError {
    GitHubError::Request { url: url.to_string(), source: Box::new(source) }
}
