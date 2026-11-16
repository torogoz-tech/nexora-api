use serde::Deserialize;
#[derive(Deserialize, Default)]
pub(crate) struct TaskDocumentQuery {
    pub project_id: Option<String>,
    pub search: Option<String>,
    pub after: Option<String>,
}
