use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskTemplateVersionsQuery {
    pub before: Option<i64>,
    pub limit: Option<u16>,
}
