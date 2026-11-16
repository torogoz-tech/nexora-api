use nexora_platform::Capability;
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrantRequest {
    pub capability: Capability,
    pub project_id: Option<String>,
}
