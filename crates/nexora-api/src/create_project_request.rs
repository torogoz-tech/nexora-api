use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateProjectRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<nexora_platform::HexColor>,
    pub description: Option<String>,
}
