use serde::Deserialize;
use serde_json::Value;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpAnalysisProfileValidate {
    pub package: Value,
}
