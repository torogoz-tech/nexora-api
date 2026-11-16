use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PolicyPredicate {
    Attribute(crate::AttributePredicate),
    Relation(crate::RelationPredicate),
}
