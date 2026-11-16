use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BranchProtectionPreview {
    #[serde(default)]
    pub required_checks: Vec<String>,
}
