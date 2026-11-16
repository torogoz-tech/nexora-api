use serde::Deserialize;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeContributionQuery {
    pub project_id: Option<String>,
    pub kind: Option<String>,
    pub search: Option<String>,
    pub after: Option<String>,
    pub revision: Option<i64>,
    pub descending: Option<bool>,
}
