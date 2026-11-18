use crate::{RabbitMqJobWakeupConsumer, RepositoryWorker, RepositoryWorkerSettings};
use nexora_platform::{JobKind, PlatformError};
use nexora_platform_store::GitNetworkPolicy;
use nexora_runtime::DatabaseConnections;
use std::{path::PathBuf, sync::mpsc, time::Duration};
use tokio::sync::oneshot;

pub struct RepositoryWorkerHost;

impl RepositoryWorkerHost {
    pub async fn run(
        config: DatabaseConnections,
        schema: String,
        settings: PathBuf,
        policy: Option<PathBuf>,
        once: bool,
    ) -> Result<(), PlatformError> {
        if once {
            return Self::run_once(config, schema, settings, policy).await;
        }

        let uri = std::env::var("NEXORA_RABBITMQ_URL").map_err(|_| {
            PlatformError::Validation(
                "NEXORA_RABBITMQ_URL is required for continuous worker execution",
            )
        })?;
        if uri.trim().is_empty() {
            return Err(PlatformError::Validation(
                "NEXORA_RABBITMQ_URL cannot be empty",
            ));
        }

        Self::run_from_rabbitmq(config, schema, settings, policy, uri).await
    }

    async fn run_once(
        config: DatabaseConnections,
        schema: String,
        settings_path: PathBuf,
        policy_path: Option<PathBuf>,
    ) -> Result<(), PlatformError> {
        tokio::task::spawn_blocking(move || {
            let settings = RepositoryWorkerSettings::load(&settings_path)?;
            let policy = policy_path
                .as_deref()
                .map(GitNetworkPolicy::load)
                .transpose()?
                .unwrap_or_default();
            let mut worker = RepositoryWorker::connect(&config, &schema, settings, policy)?;
            let result = worker.run_once()?;
            println!("{}", serde_json::json!({"job":result}));
            Ok(())
        })
        .await
        .map_err(|_| PlatformError::Unavailable)?
    }

    async fn run_from_rabbitmq(
        config: DatabaseConnections,
        schema: String,
        settings_path: PathBuf,
        policy_path: Option<PathBuf>,
        uri: String,
    ) -> Result<(), PlatformError> {
        let (wakeup_sender, wakeup_receiver) = mpsc::channel::<oneshot::Sender<bool>>();
        let (ready_sender, ready_receiver) = oneshot::channel();
        let worker_task = tokio::task::spawn_blocking(move || {
            let settings = match RepositoryWorkerSettings::load(&settings_path) {
                Ok(settings) => settings,
                Err(error) => {
                    let _ = ready_sender.send(false);
                    return Err(error);
                }
            };
            let policy = match policy_path
                .as_deref()
                .map(GitNetworkPolicy::load)
                .transpose()
            {
                Ok(policy) => policy.unwrap_or_default(),
                Err(error) => {
                    let _ = ready_sender.send(false);
                    return Err(error);
                }
            };
            let mut worker = match RepositoryWorker::connect(&config, &schema, settings, policy) {
                Ok(worker) => worker,
                Err(error) => {
                    let _ = ready_sender.send(false);
                    return Err(error);
                }
            };
            if let Err(error) = Self::drain_analysis_jobs(&mut worker) {
                let _ = ready_sender.send(false);
                return Err(error);
            }
            let _ = ready_sender.send(true);

            while let Ok(reply) = wakeup_receiver.recv() {
                match Self::drain_analysis_jobs(&mut worker) {
                    Ok(()) => {
                        let _ = reply.send(true);
                    }
                    Err(error) => {
                        let _ = reply.send(false);
                        return Err(error);
                    }
                }
            }
            Ok(())
        });

        if !ready_receiver
            .await
            .map_err(|_| PlatformError::Unavailable)?
        {
            return worker_task.await.map_err(|_| PlatformError::Unavailable)?;
        }

        let mut retry_seconds = 1;
        loop {
            let mut consumer = match RabbitMqJobWakeupConsumer::connect(
                &uri,
                JobKind::RepositoryIndex,
            )
            .await
            {
                Ok(consumer) => consumer,
                Err(_) => {
                    eprintln!(
                        "nexora-analysis-worker: RabbitMQ unavailable; database recovery completed"
                    );
                    if Self::wait_to_reconnect(retry_seconds).await {
                        drop(wakeup_sender);
                        return worker_task.await.map_err(|_| PlatformError::Unavailable)?;
                    }
                    retry_seconds = (retry_seconds * 2).min(30);
                    continue;
                }
            };
            retry_seconds = 1;

            if Self::request_analysis_drain(&wakeup_sender).await.is_err() {
                drop(wakeup_sender);
                return worker_task.await.map_err(|_| PlatformError::Unavailable)?;
            }

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        drop(wakeup_sender);
                        return worker_task.await.map_err(|_| PlatformError::Unavailable)?;
                    }
                    result = consumer.next() => {
                        let delivery = match result {
                            Ok(Some(delivery)) => delivery,
                            Ok(None) | Err(_) => {
                                eprintln!("nexora-analysis-worker: RabbitMQ consumer disconnected");
                                break;
                            }
                        };
                        if Self::request_analysis_drain(&wakeup_sender).await.is_ok() {
                            delivery.ack().await?;
                        } else {
                            delivery.retry().await?;
                            return Err(PlatformError::Unavailable);
                        }
                    }
                }
            }

            if Self::wait_to_reconnect(retry_seconds).await {
                drop(wakeup_sender);
                return worker_task.await.map_err(|_| PlatformError::Unavailable)?;
            }
            retry_seconds = (retry_seconds * 2).min(30);
        }
    }

    fn drain_analysis_jobs(worker: &mut RepositoryWorker) -> Result<(), PlatformError> {
        while worker.run_analysis_once()?.is_some() {}
        Ok(())
    }

    async fn request_analysis_drain(
        wakeup_sender: &mpsc::Sender<oneshot::Sender<bool>>,
    ) -> Result<(), PlatformError> {
        let (reply, completed) = oneshot::channel();
        wakeup_sender
            .send(reply)
            .map_err(|_| PlatformError::Unavailable)?;
        match completed.await {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(PlatformError::Unavailable),
        }
    }

    async fn wait_to_reconnect(seconds: u64) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(seconds)) => false,
            _ = tokio::signal::ctrl_c() => true,
        }
    }
}
