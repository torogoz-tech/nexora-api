use nexora_platform::TaskTransitionRequest;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskTransition {
    #[serde(default)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub transition_id: String,
    pub request_id: String,
    pub parameters: TaskTransitionRequest,
}
