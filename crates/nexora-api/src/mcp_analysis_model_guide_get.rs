use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpAnalysisModelGuideGet {
    pub component: Option<String>,
    pub version: Option<u32>,
}
