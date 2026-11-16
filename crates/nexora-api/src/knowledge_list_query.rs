use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeListQuery {
    pub snapshot: Option<String>,
    pub status: Option<String>,
    pub node: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}
