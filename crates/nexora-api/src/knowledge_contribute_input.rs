use nexora_runtime::KnowledgeDraft;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeContributeInput {
    pub snapshot: Option<String>,
    pub draft: KnowledgeDraft,
}
