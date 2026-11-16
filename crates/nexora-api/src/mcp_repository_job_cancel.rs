use nexora_platform::RepositoryJobChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRepositoryJobCancel {
    pub project_id: String,
    pub job_id: String,
    pub request_id: String,
    pub change: RepositoryJobChange,
}
