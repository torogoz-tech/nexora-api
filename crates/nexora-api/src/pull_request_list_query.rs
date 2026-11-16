use serde::Deserialize;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct PullRequestListQuery {
    pub state: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u16>,
}
