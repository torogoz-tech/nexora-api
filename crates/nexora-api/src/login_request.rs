use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginRequest {
    pub username: String,
    pub password: String,
}
