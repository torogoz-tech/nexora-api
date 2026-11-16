use crate::{SourceError, git_disk_budget::GitDiskBudget, owned_process_group::OwnedProcessGroup};
use std::{
    io::{Read, Write},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(crate) struct BoundedGitProcess;
impl BoundedGitProcess {
    fn capture(
        stream: impl Read + Send + 'static,
        limit: usize,
        overflow: Arc<AtomicBool>,
    ) -> Result<JoinHandle<Result<Vec<u8>, SourceError>>, SourceError> {
        let take = u64::try_from(limit)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(SourceError::LimitExceeded)?;
        std::thread::Builder::new()
            .name("nexora-source-pipe".into())
            .spawn(move || {
                let mut bytes = Vec::new();
                stream
                    .take(take)
                    .read_to_end(&mut bytes)
                    .map_err(|_| SourceError::Unavailable)?;
                if bytes.len() > limit {
                    overflow.store(true, Ordering::Release);
                }
                Ok(bytes)
            })
            .map_err(|_| SourceError::Unavailable)
    }
    pub fn run(
        command: &mut Command,
        timeout: Duration,
        limit: usize,
    ) -> Result<Vec<u8>, SourceError> {
        Self::run_input(command, timeout, limit, None)
    }
    pub fn run_input(
        command: &mut Command,
        timeout: Duration,
        limit: usize,
        input: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, SourceError> {
        Self::run_control(command, timeout, limit, input, None, None)
    }
    pub fn run_disk(
        command: &mut Command,
        timeout: Duration,
        limit: usize,
        disk: &GitDiskBudget,
        max_file_bytes: u64,
    ) -> Result<Vec<u8>, SourceError> {
        Self::run_control(
            command,
            timeout,
            limit,
            None,
            Some(disk),
            Some(max_file_bytes),
        )
    }
    fn run_control(
        command: &mut Command,
        timeout: Duration,
        limit: usize,
        input: Option<Vec<u8>>,
        disk: Option<&GitDiskBudget>,
        max_file_bytes: Option<u64>,
    ) -> Result<Vec<u8>, SourceError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(SourceError::InvalidConfiguration)?;
        if let Some(disk) = disk {
            disk.check()?;
        }
        let mut group = OwnedProcessGroup::spawn(command, max_file_bytes)?;
        let overflow = Arc::new(AtomicBool::new(false));
        let stdout = Self::capture(
            group.child.stdout.take().ok_or(SourceError::Unavailable)?,
            limit,
            overflow.clone(),
        )?;
        let stderr = Self::capture(
            group.child.stderr.take().ok_or(SourceError::Unavailable)?,
            65_536,
            overflow.clone(),
        )?;
        let writer = if let Some(input) = input {
            let mut stdin = group.child.stdin.take().ok_or(SourceError::Unavailable)?;
            Some(
                std::thread::Builder::new()
                    .name("nexora-source-input".into())
                    .spawn(move || {
                        stdin
                            .write_all(&input)
                            .map_err(|_| SourceError::Unavailable)
                    })
                    .map_err(|_| SourceError::Unavailable)?,
            )
        } else {
            None
        };
        let mut failure = None;
        let mut last_disk = Instant::now();
        loop {
            if overflow.load(Ordering::Acquire) {
                failure = Some(SourceError::LimitExceeded);
                break;
            }
            if Instant::now() >= deadline {
                failure = Some(SourceError::TimedOut);
                break;
            }
            if group.exited()? {
                break;
            }
            if last_disk.elapsed() >= Duration::from_millis(100) {
                if let Some(disk) = disk
                    && let Err(error) = disk.check()
                {
                    failure = Some(error);
                    break;
                }
                last_disk = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let status = group.stop()?;
        let bytes = stdout.join().map_err(|_| SourceError::Unavailable)??;
        let _discarded_stderr = stderr.join().map_err(|_| SourceError::Unavailable)??;
        let input_result = writer
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| SourceError::Unavailable)
                    .and_then(|result| result)
            })
            .unwrap_or(Ok(()));
        if let Some(error) = failure {
            return Err(error);
        }
        if overflow.load(Ordering::Acquire) {
            return Err(SourceError::LimitExceeded);
        }
        if !status.success() {
            return Err(SourceError::Unavailable);
        }
        input_result?;
        if let Some(disk) = disk {
            disk.check()?;
        }
        Ok(bytes)
    }
}
