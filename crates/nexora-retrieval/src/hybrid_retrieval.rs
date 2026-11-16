use nexora_core::{NexoraError, NodeId};
/// Future ranked fusion contract; the bootstrap does not claim hybrid scoring.
pub trait HybridRetrieval {
    fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<NodeId>, NexoraError>;
}
