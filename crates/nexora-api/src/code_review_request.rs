use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CodeReviewRequest {
    pub expected_head_sha: String,
}
