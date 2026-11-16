use crate::{RepositoryWorker, RepositoryWorkerSettings};
use nexora_platform::PlatformError;
use nexora_platform_store::GitNetworkPolicy;
use nexora_runtime::DatabaseConnections;
use std::{path::PathBuf, sync::mpsc, time::Duration};

pub struct RepositoryWorkerHost;
impl RepositoryWorkerHost {
    pub async fn run(
        config: DatabaseConnections,
        schema: String,
        settings: PathBuf,
        policy: Option<PathBuf>,
        once: bool,
    ) -> Result<(), PlatformError> {
        let (stop, listen) = mpsc::channel();
        let mut task = tokio::task::spawn_blocking(move || {
            let settings = RepositoryWorkerSettings::load(&settings)?;
            let policy = policy
                .as_deref()
                .map(GitNetworkPolicy::load)
                .transpose()?
                .unwrap_or_default();
            let mut worker = RepositoryWorker::connect(&config, &schema, settings, policy)?;
            loop {
                match listen.try_recv() {
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                let job = match worker.run_once() {
                    Ok(job) => job,
                    Err(PlatformError::Conflict(_)) => {
                        // Cancellation, expiry or recovery by another worker can
                        // supersede a claimed attempt. Its fenced result is not a
                        // fatal host failure; continue consuming authorized work.
                        if once {
                            println!("{}", serde_json::json!({"job":null,"superseded":true}));
                            return Ok(());
                        }
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let empty = job.is_none();
                if once || !empty {
                    println!("{}", serde_json::json!({"job":job}));
                }
                if once {
                    return Ok(());
                }
                if empty {
                    match listen.recv_timeout(Duration::from_secs(2)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            }
        });
        tokio::select! {
            result = &mut task => result.map_err(|_| PlatformError::Unavailable)?,
            _ = tokio::signal::ctrl_c() => {
                drop(stop);
                // Finish the current attempt before exiting; SQL fencing remains
                // authoritative if the process is instead forcibly terminated.
                task.await.map_err(|_| PlatformError::Unavailable)?
            }
        }
    }
}
