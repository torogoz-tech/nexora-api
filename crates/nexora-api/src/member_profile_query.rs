use serde::Deserialize;
#[derive(Deserialize)]
pub(crate) struct MemberProfileQuery {
    pub year: Option<i32>,
}
