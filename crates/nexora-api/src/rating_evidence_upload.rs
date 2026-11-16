#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RatingEvidenceUpload {
    #[serde(default)]
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub image: bool,
    pub data_base64: String,
}
