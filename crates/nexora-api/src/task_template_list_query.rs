use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskTemplateListQuery {
    pub after: Option<String>,
    pub limit: Option<u16>,
    pub include_archived: Option<bool>,
}
