use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskHistoryQuery {
    pub after: Option<i64>,
    pub limit: Option<u16>,
}
