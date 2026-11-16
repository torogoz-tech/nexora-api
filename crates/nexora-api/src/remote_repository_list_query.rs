use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteRepositoryListQuery {
    pub page: Option<u32>,
    pub limit: Option<u16>,
}
