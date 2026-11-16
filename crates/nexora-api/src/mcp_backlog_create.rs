use nexora_platform::BacklogCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpBacklogCreate {
    pub project_id: String,
    pub request_id: String,
    pub definition: BacklogCreate,
}
