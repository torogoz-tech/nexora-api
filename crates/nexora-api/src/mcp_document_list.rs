use nexora_platform::DocumentListQuery;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpDocumentList {
    pub project_id: String,
    #[serde(default)]
    pub query: DocumentListQuery,
}
