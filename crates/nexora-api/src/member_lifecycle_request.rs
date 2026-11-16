use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemberLifecycleRequest {
    pub archived: bool,
    pub expected_revision: i64,
}
