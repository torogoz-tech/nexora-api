use serde::Deserialize;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PullRequestLinksQuery {
    pub after: Option<String>,
    pub limit: Option<u16>,
}
