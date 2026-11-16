use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpTaskPoolList {
    #[serde(default)]
    pub query: nexora_platform::VisibleTaskQuery,
}
