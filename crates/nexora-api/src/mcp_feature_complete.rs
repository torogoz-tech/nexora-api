use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpFeatureComplete {
    pub project_id: String,
    pub feature_id: String,
    pub request_id: String,
    pub expected_revision: i64,
}
