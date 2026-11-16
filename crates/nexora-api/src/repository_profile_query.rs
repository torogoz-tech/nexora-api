use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryProfileQuery {
    pub year: Option<i32>,
    pub full_name: Option<String>,
}
