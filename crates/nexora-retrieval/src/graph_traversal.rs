use nexora_core::NodeId;
use nexora_graph::CodeNode;
pub trait GraphTraversal {
    fn traverse(&self, start: &NodeId, depth: usize, limit: usize) -> Vec<&CodeNode>;
}
