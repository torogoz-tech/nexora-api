use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskPoolOptions {
    pub project_id: Option<String>,
}
