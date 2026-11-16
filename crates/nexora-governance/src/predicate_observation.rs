use crate::EvaluationStatus;
use nexora_core::EdgeId;
pub struct PredicateObservation {
    pub status: EvaluationStatus,
    pub edge_ids: Vec<EdgeId>,
    pub exhaustive: bool,
    pub message: String,
}
