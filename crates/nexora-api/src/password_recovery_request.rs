use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordRecoveryRequest {
    pub email: String,
}
