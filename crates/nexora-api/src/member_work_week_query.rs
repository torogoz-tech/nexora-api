#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemberWorkWeekQuery {
    pub date: Option<String>,
}
