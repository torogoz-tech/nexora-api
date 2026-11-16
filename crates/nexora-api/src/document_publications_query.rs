use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentPublicationsQuery {
    pub after: Option<String>,
    pub limit: Option<u16>,
}
