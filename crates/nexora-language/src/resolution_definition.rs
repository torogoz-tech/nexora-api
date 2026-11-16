use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionDefinition {
    pub literal: crate::LiteralDefinition,
    #[serde(default = "ResolutionDefinition::steps")]
    pub max_steps: usize,
    pub routes: Vec<crate::ResolutionRoute>,
}

impl ResolutionDefinition {
    fn steps() -> usize {
        1_000_000
    }
}
