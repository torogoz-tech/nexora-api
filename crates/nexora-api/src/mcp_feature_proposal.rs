use nexora_platform::FeatureProposalCreate;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpFeatureProposal {
    pub project_id: String,
    pub request_id: String,
    pub proposal: FeatureProposalCreate,
}
