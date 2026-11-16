use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpEvidenceUpload {
    pub project_id: String,
    pub request_id: String,
    pub evidence: nexora_platform::EvidenceUpload,
}
