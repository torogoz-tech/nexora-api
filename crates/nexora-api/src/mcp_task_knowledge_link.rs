#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskKnowledgeLink {
    pub task_id: String,
    pub request_id: String,
    pub link: nexora_platform::TaskKnowledgeLink,
}
