use crate::{ApiState, RabbitMqJobWakeupPublisher};
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle, time::sleep};

pub struct RabbitMqJobWakeupDispatcher;

impl RabbitMqJobWakeupDispatcher {
    pub fn start(state: ApiState, uri: String) -> (mpsc::Sender<()>, JoinHandle<()>) {
        let (sender, receiver) = mpsc::channel(1);
        let task = tokio::spawn(Self::run(state, uri, receiver));
        (sender, task)
    }

    async fn run(state: ApiState, uri: String, mut receiver: mpsc::Receiver<()>) {
        let mut retry_seconds = 1;
        loop {
            let publisher = match RabbitMqJobWakeupPublisher::connect(&uri).await {
                Ok(publisher) => publisher,
                Err(_) => {
                    eprintln!(
                        "nexora-api: RabbitMQ unavailable; durable jobs remain in PostgreSQL"
                    );
                    if !Self::wait_for_retry(&mut receiver, retry_seconds).await {
                        return;
                    }
                    retry_seconds = (retry_seconds * 2).min(30);
                    continue;
                }
            };

            if Self::publish_pending(&state, &publisher).await.is_err() {
                eprintln!("nexora-api: RabbitMQ dispatch failed; retrying from PostgreSQL");
                if !Self::wait_for_retry(&mut receiver, retry_seconds).await {
                    return;
                }
                retry_seconds = (retry_seconds * 2).min(30);
                continue;
            }

            retry_seconds = 1;
            loop {
                tokio::select! {
                    signal = receiver.recv() => {
                        if signal.is_none() {
                            return;
                        }
                        if Self::publish_pending(&state, &publisher).await.is_err() {
                            eprintln!("nexora-api: RabbitMQ dispatch failed; retrying from PostgreSQL");
                            break;
                        }
                    }
                    _ = sleep(Duration::from_secs(5)) => {
                        if !publisher.is_connected() {
                            eprintln!("nexora-api: RabbitMQ connection closed; retrying from PostgreSQL");
                            break;
                        }
                    }
                }
            }
            if !Self::wait_for_retry(&mut receiver, retry_seconds).await {
                return;
            }
            retry_seconds = (retry_seconds * 2).min(30);
        }
    }

    async fn wait_for_retry(receiver: &mut mpsc::Receiver<()>, seconds: u64) -> bool {
        tokio::select! {
            _ = sleep(Duration::from_secs(seconds)) => true,
            signal = receiver.recv() => {
                signal.is_some()
            }
        }
    }

    async fn publish_pending(
        state: &ApiState,
        publisher: &RabbitMqJobWakeupPublisher,
    ) -> Result<(), ()> {
        let jobs = state
            .run(|store| store.pending_job_wakeups())
            .await
            .map_err(|_| ())?;
        for wakeup in jobs {
            publisher.publish(&wakeup).await.map_err(|_| ())?;
        }
        Ok(())
    }
}
