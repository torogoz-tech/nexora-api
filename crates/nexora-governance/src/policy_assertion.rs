use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PolicyAssertion {
    Group(crate::AssertionGroup),
    Predicate(crate::PolicyPredicate),
}
