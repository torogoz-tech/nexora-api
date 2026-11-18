use crate::PlatformStore;
use nexora_platform::{JobKind, JobWakeup, PlatformError, RepositoryJobKind};

pub(crate) struct PendingJobWakeups;

impl PendingJobWakeups {
    pub(crate) fn list(store: &mut PlatformStore) -> Result<Vec<JobWakeup>, PlatformError> {
        let mut wakeups = store
            .pending_repository_jobs_for_wakeup()?
            .into_iter()
            .map(|job| {
                let kind = match job.kind {
                    RepositoryJobKind::Sync => JobKind::RepositorySync,
                    RepositoryJobKind::Index => JobKind::RepositoryIndex,
                };
                JobWakeup::new(job.id, kind).map_err(|_| PlatformError::Storage)
            })
            .collect::<Result<Vec<_>, _>>()?;

        for (kind, query) in [
            (
                JobKind::TaskTransitionResume,
                "SELECT id FROM task_transition_runs WHERE record->>'status'='ready' OR (record->>'status'='waiting_external' AND updated_at<clock_timestamp()-interval '30 seconds') ORDER BY updated_at,id LIMIT 1",
            ),
            (
                JobKind::ReleaseComposition,
                "SELECT id FROM task_release_composition_jobs WHERE status='queued' OR (status='running' AND updated_at<clock_timestamp()-interval '2 minutes') ORDER BY created_at,id LIMIT 1",
            ),
            (
                JobKind::KnowledgeSynthesis,
                "SELECT id FROM knowledge_contributions WHERE status='queued' ORDER BY created_at,id LIMIT 1",
            ),
            (
                JobKind::KnowledgeProcessing,
                "SELECT id FROM knowledge_processing_jobs WHERE (status='queued' OR (status='processing' AND lease_until<=clock_timestamp())) AND attempts<3 ORDER BY created_at,id LIMIT 1",
            ),
            (
                JobKind::DailyDeveloperAnalysis,
                "SELECT actor_id||':'||date::text FROM daily_developer_analyses WHERE status='pending' ORDER BY date,actor_id LIMIT 1",
            ),
            (
                JobKind::EmailNotification,
                "SELECT event_key FROM email_notifications WHERE status IN ('pending','delivering') AND attempts<5 AND available_at<=clock_timestamp() ORDER BY available_at,event_key LIMIT 1",
            ),
        ] {
            if let Some(row) = store
                .client
                .query_opt(query, &[])
                .map_err(PlatformStore::database_error)?
            {
                let id: String = row.get(0);
                wakeups.push(JobWakeup::new(id, kind).map_err(|_| PlatformError::Storage)?);
            }
        }

        Ok(wakeups)
    }
}
