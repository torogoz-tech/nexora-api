use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TraversalLimit {
    #[serde(default = "TraversalLimit::depth")]
    pub max_depth: usize,
    #[serde(default = "TraversalLimit::visits")]
    pub max_visits: usize,
}
impl TraversalLimit {
    fn depth() -> usize {
        1
    }
    fn visits() -> usize {
        1000
    }
}
impl Default for TraversalLimit {
    fn default() -> Self {
        Self {
            max_depth: 1,
            max_visits: 1000,
        }
    }
}
