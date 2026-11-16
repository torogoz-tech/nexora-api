use crate::{
    FetchedGitSource, GitFetchCredentials, GitFetchPolicy, GitObjectFormat, GitTransferLimits,
    SourceError, git_disk_budget::GitDiskBudget, source_private_directory::SourcePrivateDirectory,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};
pub struct GitFetcher {
    binary: PathBuf,
    limits: GitTransferLimits,
}
impl GitFetcher {
    pub fn new(binary: PathBuf, limits: GitTransferLimits) -> Result<Self, SourceError> {
        limits.validate()?;
        if !binary.is_absolute() || !binary.is_file() {
            return Err(SourceError::InvalidConfiguration);
        }
        Ok(Self { binary, limits })
    }
    fn command(&self, root: &Path) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .current_dir(root)
            .env_clear()
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args([
                "--no-pager",
                "--no-replace-objects",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "credential.helper=",
                "-c",
                "http.proxy=",
                "-c",
                "http.followRedirects=false",
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "protocol.http.allow=never",
                "-c",
                "fetch.fsckObjects=true",
                "-c",
                "transfer.fsckObjects=true",
                "-c",
                "fetch.unpackLimit=0",
                "-c",
                "fetch.recurseSubmodules=false",
                "-c",
                "fetch.writeCommitGraph=false",
                "-c",
                "gc.auto=0",
                "-c",
                "pack.threads=1",
                "-c",
                "index.threads=1",
                "-c",
                "core.deltaBaseCacheLimit=16m",
                "-c",
                "core.bigFileThreshold=16m",
                "-c",
                "fetch.uriprotocols=",
                "-c",
                "promisor.acceptFromServer=none",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    pub fn fetch(
        &self,
        policy: &GitFetchPolicy,
        repository: &str,
        commit: &str,
        format: GitObjectFormat,
        credentials: &GitFetchCredentials,
        destination: &Path,
    ) -> Result<FetchedGitSource, SourceError> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (policy, repository, commit, format, credentials, destination);
            return Err(SourceError::UnsupportedHost);
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use crate::bounded_git_process::BoundedGitProcess;
            format.validate_oid(commit)?;
            let remote = policy.repository(repository)?;
            let deadline = Instant::now()
                .checked_add(self.limits.timeout)
                .ok_or(SourceError::InvalidConfiguration)?;
            let remaining = || {
                deadline
                    .checked_duration_since(Instant::now())
                    .filter(|time| !time.is_zero())
                    .ok_or(SourceError::TimedOut)
            };
            let directory = SourcePrivateDirectory::create(destination)?;
            let bare = directory.path().join("objects.git");
            let mut initialize = self.command(directory.path());
            let object_format = match format {
                GitObjectFormat::Sha1 => "--object-format=sha1",
                GitObjectFormat::Sha256 => "--object-format=sha256",
            };
            initialize
                .args(["init", "--bare", "--template=", object_format])
                .arg(&bare);
            BoundedGitProcess::run(&mut initialize, remaining()?, 8192)?;
            let disk = GitDiskBudget {
                root: directory.path().into(),
                max_bytes: self.limits.max_repository_bytes,
                max_entries: self.limits.max_repository_entries,
            };
            disk.check()?;
            let mut fetch = self.command(directory.path());
            if policy.origin.scheme() == "http" {
                fetch.args(["-c", "protocol.http.allow=always"]);
            }
            fetch
                .env("GIT_CONFIG_COUNT", "1")
                .env(
                    "GIT_CONFIG_KEY_0",
                    format!("http.{}.extraHeader", policy.origin.as_str()),
                )
                .env("GIT_CONFIG_VALUE_0", credentials.header.as_str())
                .arg("-C")
                .arg(&bare)
                .args([
                    "fetch",
                    "--depth=1",
                    "--no-tags",
                    "--no-recurse-submodules",
                    "--no-auto-maintenance",
                    "--no-write-fetch-head",
                    "--no-write-commit-graph",
                    "--keep",
                    "--quiet",
                    "--no-progress",
                ])
                .arg(remote.as_str())
                .arg(commit);
            BoundedGitProcess::run_disk(
                &mut fetch,
                remaining()?,
                8192,
                &disk,
                self.limits.max_pack_bytes,
            )?;
            let mut check = self.command(directory.path());
            check.arg("-C").arg(&bare).args(["cat-file", "-t", commit]);
            if BoundedGitProcess::run(&mut check, remaining()?, 32)? != b"commit\n" {
                return Err(SourceError::InvalidObject);
            }
            let repository_bytes = disk.check()?;
            let root = directory.retain();
            Ok(FetchedGitSource {
                bare: root.join("objects.git"),
                commit: commit.into(),
                repository_bytes,
            })
        }
    }
}
impl GitFetcher {
    pub fn fetch_into(
        &self,
        policy: &GitFetchPolicy,
        repository: &str,
        commit: &str,
        format: GitObjectFormat,
        credentials: &GitFetchCredentials,
        bare: &Path,
    ) -> Result<(), SourceError> {
        format.validate_oid(commit)?;
        let remote = policy.repository(repository)?;
        let mut command = self.command(bare.parent().ok_or(SourceError::InvalidConfiguration)?);
        if policy.origin.scheme() == "http" {
            command.args(["-c", "protocol.http.allow=always"]);
        }
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env(
                "GIT_CONFIG_KEY_0",
                format!("http.{}.extraHeader", policy.origin.as_str()),
            )
            .env("GIT_CONFIG_VALUE_0", credentials.header.as_str())
            .arg("-C")
            .arg(bare)
            .args([
                "fetch",
                "--depth=2",
                "--no-tags",
                "--no-recurse-submodules",
                "--no-auto-maintenance",
                "--no-write-fetch-head",
                "--quiet",
                "--no-progress",
            ])
            .arg(remote.as_str())
            .arg(commit);
        let disk = GitDiskBudget {
            root: bare.parent().unwrap().to_owned(),
            max_bytes: self.limits.max_repository_bytes,
            max_entries: self.limits.max_repository_entries,
        };
        crate::bounded_git_process::BoundedGitProcess::run_disk(
            &mut command,
            self.limits.timeout,
            8192,
            &disk,
            self.limits.max_pack_bytes,
        )
        .map(|_| ())
    }
}
