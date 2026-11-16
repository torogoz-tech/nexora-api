use serde::Deserialize;
#[derive(Deserialize)]
pub(crate) struct OrganizationKnowledgeQuery {
    pub kind: Option<String>,
    pub search: Option<String>,
    pub after: Option<String>,
    pub revision: Option<i64>,
}
