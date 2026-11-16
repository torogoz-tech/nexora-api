use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GithubPageQuery {
    pub page: Option<u32>,
    pub limit: Option<u16>,
}
