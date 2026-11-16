use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationPredicate {
    pub relation: String,
    #[serde(default)]
    pub target: crate::PolicyRelationTarget,
    #[serde(default)]
    pub traversal: crate::TraversalLimit,
}
