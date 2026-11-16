#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeImageUpload {
    pub project_id: Option<String>,
    pub name: String,
    pub image_base64: String,
}
