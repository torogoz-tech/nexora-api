use nexora_platform::RepositoryChange;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRepositoryChange {
    pub project_id: String,
    pub repository_id: String,
    pub change: RepositoryChange,
    pub request_id: String,
}
