use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PolicySpec {
    pub target: crate::PolicyTarget,
    pub assert: crate::PolicyAssertion,
    #[serde(default)]
    pub on_unknown: crate::UnknownBehavior,
    #[serde(default)]
    pub scope: crate::PolicyScope,
    #[serde(default)]
    pub requires: Vec<crate::PolicyReference>,
}
