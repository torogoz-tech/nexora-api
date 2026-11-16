#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskEvidenceUpload {
    pub name: String,
    pub image_base64: String,
}
