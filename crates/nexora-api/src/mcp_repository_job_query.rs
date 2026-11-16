use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRepositoryJobQuery {
    pub project_id: String,
    pub job_id: String,
}
