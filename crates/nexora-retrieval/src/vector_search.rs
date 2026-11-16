use nexora_core::{NexoraError, NodeId};
/// Future vector backend contract. No embedding generation or implementation in bootstrap.
pub trait VectorSearch {
    fn vector_search(&self, embedding: &[f32], limit: usize) -> Result<Vec<NodeId>, NexoraError>;
}
