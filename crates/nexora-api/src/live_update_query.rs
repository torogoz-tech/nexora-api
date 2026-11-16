use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LiveUpdateQuery {
    pub project_id: Option<String>,
}
