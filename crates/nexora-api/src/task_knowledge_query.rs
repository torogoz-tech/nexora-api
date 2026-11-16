#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskKnowledgeQuery {
    pub kind: String,
    pub after: Option<String>,
}
