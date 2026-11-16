use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentityGateRequest {
    pub action: String,
    #[serde(default)]
    pub code: String,
}
