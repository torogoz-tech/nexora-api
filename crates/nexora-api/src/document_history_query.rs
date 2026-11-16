use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentHistoryQuery {
    pub after: Option<i64>,
    pub limit: Option<u16>,
}
