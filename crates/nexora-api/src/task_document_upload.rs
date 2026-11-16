use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskDocumentUpload {
    pub name: String,
    pub data_base64: String,
}
