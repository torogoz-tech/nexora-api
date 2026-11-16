use nexora_platform::{RepositoryBinding, RepositoryJob, RepositoryJobState};
use serde::Serialize;

#[derive(Serialize)]
pub struct RepositoryWorkerOutcome {
    pub id: String,
    pub state: RepositoryJobState,
    pub pinned_commit: Option<String>,
    pub snapshot_id: Option<String>,
    pub error_code: Option<String>,
}
impl RepositoryWorkerOutcome {
    pub(crate) fn published(binding: RepositoryBinding) -> Self {
        Self {
            id: binding.job_id,
            state: RepositoryJobState::Succeeded,
            pinned_commit: Some(binding.pinned_commit),
            snapshot_id: Some(binding.snapshot_id),
            error_code: None,
        }
    }
}
impl From<RepositoryJob> for RepositoryWorkerOutcome {
    fn from(job: RepositoryJob) -> Self {
        Self {
            id: job.id,
            state: job.state,
            pinned_commit: job.pinned_commit,
            snapshot_id: job.snapshot_id,
            error_code: job.error_code,
        }
    }
}
