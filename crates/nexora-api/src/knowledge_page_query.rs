use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgePageQuery {
    pub snapshot: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}
