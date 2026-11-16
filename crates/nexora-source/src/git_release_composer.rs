use crate::{
    GitFetchCredentials, GitFetchPolicy, GitTransferLimits, SourceError,
    bounded_git_process::BoundedGitProcess, git_disk_budget::GitDiskBudget,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};
/// Builds a release tree from an explicit ordered patch set, without checking out repository files.
pub struct GitReleaseComposer {
    binary: PathBuf,
    limits: GitTransferLimits,
}
impl GitReleaseComposer {
    pub fn new(binary: PathBuf, limits: GitTransferLimits) -> Result<Self, SourceError> {
        limits.validate()?;
        if !binary.is_absolute() || !binary.is_file() {
            return Err(SourceError::InvalidConfiguration);
        }
        Ok(Self { binary, limits })
    }
    fn command(&self, bare: &Path, index: &Path) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .env_clear()
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_INDEX_FILE", index)
            .env("GIT_AUTHOR_NAME", "Nexora release")
            .env("GIT_AUTHOR_EMAIL", "release@localhost")
            .env("GIT_COMMITTER_NAME", "Nexora release")
            .env("GIT_COMMITTER_EMAIL", "release@localhost")
            .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
            .args([
                "--no-pager",
                "--no-replace-objects",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "credential.helper=",
                "-c",
                "core.fsmonitor=false",
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
                "gc.auto=0",
                "-c",
                "pack.threads=1",
                "-C",
            ])
            .arg(bare)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn oid(value: &str) -> bool {
        [40, 64].contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
    }
    pub fn compose(
        &self,
        bare: &Path,
        base: &str,
        commits: &[String],
        message: &str,
    ) -> Result<String, SourceError> {
        if !bare.is_absolute()
            || !Self::oid(base)
            || commits.is_empty()
            || commits.len() > 100
            || commits.iter().any(|sha| !Self::oid(sha))
            || message.is_empty()
            || message.len() > 4096
        {
            return Err(SourceError::InvalidConfiguration);
        }
        let index = bare
            .parent()
            .ok_or(SourceError::InvalidConfiguration)?
            .join("release.index");
        if index.exists() {
            std::fs::remove_file(&index).map_err(|_| SourceError::Unavailable)?;
        }
        let deadline = Instant::now() + self.limits.timeout;
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .ok_or(SourceError::TimedOut)
        };
        let disk = GitDiskBudget {
            root: bare.parent().unwrap().to_owned(),
            max_bytes: self.limits.max_repository_bytes,
            max_entries: self.limits.max_repository_entries,
        };
        BoundedGitProcess::run(
            self.command(bare, &index).args(["read-tree", base]),
            remaining()?,
            8192,
        )?;
        let mut seen = std::collections::BTreeSet::new();
        for sha in commits {
            if !seen.insert(sha) {
                return Err(SourceError::InvalidConfiguration);
            }
            let parents = BoundedGitProcess::run(
                self.command(bare, &index).args(["cat-file", "commit", sha]),
                remaining()?,
                4096,
            )?;
            let parents: Vec<String> = parents
                .split(|byte| *byte == b'\n')
                .take_while(|line| !line.is_empty())
                .filter_map(|line| line.strip_prefix(b"parent "))
                .map(|bytes| {
                    String::from_utf8(bytes.to_vec()).map_err(|_| SourceError::InvalidObject)
                })
                .collect::<Result<_, _>>()?;
            if parents.len() != 1 {
                return Err(SourceError::InvalidObject);
            }
            let patch = BoundedGitProcess::run(
                self.command(bare, &index).args([
                    "diff-tree",
                    "--no-commit-id",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary",
                    "--full-index",
                    "-p",
                    &parents[0],
                    sha,
                    "--",
                ]),
                remaining()?,
                16 * 1024 * 1024,
            )?;
            if patch.is_empty() {
                return Err(SourceError::InvalidObject);
            }
            BoundedGitProcess::run_input(
                self.command(bare, &index).args([
                    "apply",
                    "--cached",
                    "--3way",
                    "--whitespace=nowarn",
                    "-",
                ]),
                remaining()?,
                8192,
                Some(patch),
            )?;
            disk.check()?;
        }
        let tree = BoundedGitProcess::run(
            self.command(bare, &index).arg("write-tree"),
            remaining()?,
            128,
        )?;
        let tree = String::from_utf8(tree).map_err(|_| SourceError::InvalidObject)?;
        let commit = BoundedGitProcess::run_input(
            self.command(bare, &index)
                .args(["commit-tree", tree.trim(), "-p", base]),
            remaining()?,
            128,
            Some(message.as_bytes().to_vec()),
        )?;
        let commit = String::from_utf8(commit)
            .map_err(|_| SourceError::InvalidObject)?
            .trim()
            .to_owned();
        if !Self::oid(&commit) {
            return Err(SourceError::InvalidObject);
        }
        Ok(commit)
    }
    pub fn push_new(
        &self,
        bare: &Path,
        policy: &GitFetchPolicy,
        repository: &str,
        credentials: &GitFetchCredentials,
        commit: &str,
        branch: &str,
    ) -> Result<(), SourceError> {
        if !Self::oid(commit)
            || !branch.starts_with("release/")
            || branch.contains("..")
            || branch
                .chars()
                .any(|c| c.is_whitespace() || "~^:?*[\\".contains(c))
        {
            return Err(SourceError::InvalidConfiguration);
        }
        let remote = policy.repository(repository)?;
        let index = bare
            .parent()
            .ok_or(SourceError::InvalidConfiguration)?
            .join("release.index");
        let mut command = self.command(bare, &index);
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
            .args(["push", "--quiet", "--atomic"])
            .arg(format!("--force-with-lease=refs/heads/{branch}:"))
            .arg(remote.as_str())
            .arg(format!("{commit}:refs/heads/{branch}"));
        let disk = GitDiskBudget {
            root: bare
                .parent()
                .ok_or(SourceError::InvalidConfiguration)?
                .to_owned(),
            max_bytes: self.limits.max_repository_bytes,
            max_entries: self.limits.max_repository_entries,
        };
        BoundedGitProcess::run_disk(
            &mut command,
            self.limits.timeout,
            8192,
            &disk,
            self.limits.max_pack_bytes,
        )
        .map(|_| ())
    }
}
