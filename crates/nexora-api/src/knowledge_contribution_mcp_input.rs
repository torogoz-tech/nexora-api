use nexora_platform::KnowledgeContributionInput;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeContributionMcpInput {
    pub request_id: Option<String>,
    pub contribution_id: Option<String>,
    pub contribution: Option<KnowledgeContributionInput>,
    #[serde(flatten)]
    pub query: crate::knowledge_contribution_query::KnowledgeContributionQuery,
}
