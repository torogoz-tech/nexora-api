use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeReviewInput {
    pub snapshot: Option<String>,
    pub expected_revision: u32,
    pub action: String,
    pub reason: String,
}
