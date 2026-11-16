use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidencePageQuery {
    pub after: Option<String>,
    pub limit: Option<u16>,
}
