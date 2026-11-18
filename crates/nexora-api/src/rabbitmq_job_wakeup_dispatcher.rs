use crate::{ApiState, RabbitMqJobWakeupPublisher};
use nexora_platform::{JobKind, JobWakeup, PlatformError};
use nexora_task_workflow::TaskKnowledgeWakeup;
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
        let mut jobs = state
            .run(|store| store.pending_job_wakeups())
            .await
            .map_err(|_| ())?;
        let knowledge_jobs = state
            .run_task_workflow(|store| store.pending_task_knowledge_wakeups())
            .await
            .map_err(|_| ())?;
        jobs.extend(
            knowledge_jobs
                .iter()
                .map(Self::map_task_knowledge_wakeup)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ())?,
        );
        for wakeup in jobs {
            publisher.publish(&wakeup).await.map_err(|_| ())?;
        }
        Ok(())
    }

    fn map_task_knowledge_wakeup(wakeup: &TaskKnowledgeWakeup) -> Result<JobWakeup, PlatformError> {
        match wakeup.job_kind.as_str() {
            "task_review" | "contributor_rating" => {
                JobWakeup::new(wakeup.job_id.clone(), JobKind::TaskReview)
                    .map_err(|_| PlatformError::Validation("invalid task knowledge job"))
            }
            _ => Err(PlatformError::Validation("unknown task knowledge job kind")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RabbitMqJobWakeupDispatcher;
    use nexora_platform::JobKind;
    use nexora_task_workflow::TaskKnowledgeWakeup;
    use std::time::SystemTime;

    #[test]
    fn review_and_rating_wakeups_share_the_task_review_route() {
        for job_kind in ["task_review", "contributor_rating"] {
            let wakeup = TaskKnowledgeWakeup {
                job_id: format!("job_{job_kind}"),
                job_kind: job_kind.to_owned(),
                status: "queued".to_owned(),
                run_id: "run_1".to_owned(),
                task_id: "task_1".to_owned(),
                available_at: SystemTime::now(),
            };

            let message = RabbitMqJobWakeupDispatcher::map_task_knowledge_wakeup(&wakeup)
                .expect("valid task knowledge job");

            assert_eq!(message.kind(), JobKind::TaskReview);
            assert_eq!(message.kind().routing_key(), "nexora.jobs.knowledge");
            assert_eq!(message.job_id(), wakeup.job_id);
        }
    }

    #[test]
    fn unknown_task_knowledge_jobs_are_not_published() {
        let wakeup = TaskKnowledgeWakeup {
            job_id: "job_unknown".to_owned(),
            job_kind: "other".to_owned(),
            status: "queued".to_owned(),
            run_id: "run_1".to_owned(),
            task_id: "task_1".to_owned(),
            available_at: SystemTime::now(),
        };

        assert!(RabbitMqJobWakeupDispatcher::map_task_knowledge_wakeup(&wakeup).is_err());
    }
}
