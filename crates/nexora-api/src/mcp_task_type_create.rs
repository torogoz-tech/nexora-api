use nexora_platform::TaskTypeCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTypeCreate {
    pub project_id: String,
    pub request_id: String,
    pub definition: TaskTypeCreate,
}
