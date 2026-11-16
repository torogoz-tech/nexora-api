use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationLogoInput {
    pub expected_revision: i64,
    pub data_base64: Option<String>,
}
