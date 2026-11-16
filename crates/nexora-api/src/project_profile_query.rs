use serde::Deserialize;
#[derive(Deserialize)]
pub(crate) struct ProjectProfileQuery {
    pub year: Option<i32>,
}
