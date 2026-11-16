use nexora_runtime::KnowledgeDraft;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeReviseInput {
    pub snapshot: Option<String>,
    pub draft: KnowledgeDraft,
    pub expected_revision: u32,
}
