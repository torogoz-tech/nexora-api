use crate::{
    GitBatchDecoder, GitBlobBatch, GitObjectFormat, GitTreeDecoder, GitTreeEntry, SourceError,
    SourceLimits,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/// Reads only objects in a server-managed bare repository. No checkout, filters or lazy fetch.
pub struct GitObjectReader {
    binary: PathBuf,
    timeout: Duration,
}
impl GitObjectReader {
    pub fn blobs(
        &self,
        bare: &Path,
        entries: &[GitTreeEntry],
        format: GitObjectFormat,
        limits: &SourceLimits,
    ) -> Result<GitBlobBatch, SourceError> {
        let limit = GitBatchDecoder::output_limit(entries, format, limits)?;
        if entries.is_empty() {
            return GitBatchDecoder::decode(Vec::new(), entries, format, limits);
        }
        let mut input = Vec::with_capacity(entries.len() * (format.oid_length() + 1));
        for entry in entries {
            input.extend_from_slice(entry.object_id.as_bytes());
            input.push(0);
        }
        let mut command = self.command(bare)?;
        command
            .args(["cat-file", "--batch", "-Z"])
            .stdin(Stdio::piped());
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let bytes = crate::bounded_git_process::BoundedGitProcess::run_input(
            &mut command,
            self.timeout,
            limit,
            Some(input),
        )?;
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let bytes = {
            let _ = (command, input, limit);
            return Err(SourceError::UnsupportedHost);
        };
        GitBatchDecoder::decode(bytes, entries, format, limits)
    }
    pub fn new(binary: PathBuf, timeout: Duration) -> Result<Self, SourceError> {
        if !binary.is_absolute()
            || !binary.is_file()
            || timeout.is_zero()
            || timeout > Duration::from_secs(120)
        {
            return Err(SourceError::InvalidConfiguration);
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        return Err(SourceError::UnsupportedHost);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Ok(Self { binary, timeout })
    }
    fn command(&self, bare: &Path) -> Result<Command, SourceError> {
        if !bare.is_absolute()
            || !std::fs::symlink_metadata(bare)
                .map_err(|_| SourceError::Unavailable)?
                .is_dir()
        {
            return Err(SourceError::InvalidConfiguration);
        }
        let mut command = Command::new(&self.binary);
        command
            .env_clear()
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ATTR_NOSYSTEM", "1")
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
                "protocol.allow=never",
            ])
            .arg("-C")
            .arg(bare)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok(command)
    }
    fn run(&self, command: &mut Command, limit: usize) -> Result<Vec<u8>, SourceError> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        return crate::bounded_git_process::BoundedGitProcess::run(command, self.timeout, limit);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (command, limit);
            Err(SourceError::UnsupportedHost)
        }
    }
    pub fn tree(
        &self,
        bare: &Path,
        commit: &str,
        format: GitObjectFormat,
        limits: &SourceLimits,
    ) -> Result<Vec<GitTreeEntry>, SourceError> {
        limits.validate()?;
        format.validate_oid(commit)?;
        let mut command = self.command(bare)?;
        command.args(["ls-tree", "-rlz", "--full-tree", commit]);
        GitTreeDecoder::decode(
            &self.run(&mut command, limits.max_tree_bytes)?,
            format,
            limits,
        )
    }
    pub fn blob(
        &self,
        bare: &Path,
        object: &str,
        format: GitObjectFormat,
        expected_size: u64,
        max_bytes: u64,
    ) -> Result<Vec<u8>, SourceError> {
        format.validate_oid(object)?;
        if expected_size > max_bytes {
            return Err(SourceError::LimitExceeded);
        }
        let limit = usize::try_from(expected_size).map_err(|_| SourceError::LimitExceeded)?;
        let mut command = self.command(bare)?;
        command.args(["cat-file", "blob", object]);
        let bytes = self.run(&mut command, limit)?;
        if bytes.len() != limit {
            return Err(SourceError::InvalidObject);
        }
        Ok(bytes)
    }
}
