use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpRepositoryQuery {
    pub project_id: String,
    pub repository_id: String,
}
