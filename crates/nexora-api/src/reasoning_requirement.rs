use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReasoningRequirement {
    pub document_id: String,
    pub version_id: String,
}
