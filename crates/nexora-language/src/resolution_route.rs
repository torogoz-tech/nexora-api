use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionRoute {
    pub accept: String,
    #[serde(default)]
    pub reject: Vec<String>,
    pub anchor: crate::ResolutionAnchor,
    #[serde(default)]
    pub rewrites: Vec<crate::TextRewrite>,
    pub candidates: Vec<crate::ResolutionCandidate>,
}
