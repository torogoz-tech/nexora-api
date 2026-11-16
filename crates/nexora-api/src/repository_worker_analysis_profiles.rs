use nexora_indexer::{AnalysisProfileCandidate, MultiProfileIndexer};
use nexora_platform::RepositoryJobFailure;
use serde_json::Value;

/// Compiles only immutable candidates frozen by the leased job. No live catalog lookup.
pub(crate) struct RepositoryWorkerAnalysisProfiles;
impl RepositoryWorkerAnalysisProfiles {
    pub fn compile(plan: Value) -> Result<MultiProfileIndexer, RepositoryJobFailure> {
        let automatic = match (plan["mode"].as_str(), plan["policy"].as_str()) {
            (Some("automatic"), Some("automatic-latest/v1")) => true,
            (Some("manual"), Some("manual/v1")) => false,
            _ => return Err(RepositoryJobFailure::ProfileUnavailable),
        };
        let candidates: Vec<AnalysisProfileCandidate> =
            serde_json::from_value(plan["candidates"].clone())
                .map_err(|_| RepositoryJobFailure::ProfileUnavailable)?;
        MultiProfileIndexer::new(candidates, automatic)
            .map_err(|_| RepositoryJobFailure::ProfileUnavailable)
    }
}

#[cfg(test)]
#[path = "repository_worker_analysis_profiles_tests.rs"]
mod tests;
