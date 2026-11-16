use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserEmailRequest {
    pub email: Option<String>,
    pub code: Option<String>,
}
