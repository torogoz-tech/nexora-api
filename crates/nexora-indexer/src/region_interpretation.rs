use nexora_core::{NodeId, RegionStatus};
use serde::{Deserialize, Serialize};
/// Connects a physical region node in one unit's graph to its interpretation unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInterpretation {
    pub source_unit: String,
    pub region_id: NodeId,
    pub target_unit: String,
    pub status: RegionStatus,
    #[serde(default)]
    pub boundary_closed: bool,
    #[serde(default)]
    pub component: nexora_core::LexicalRegionComponent,
}
