use nexora_core::NodeId;
pub enum ModuleResolution {
    Resolved(NodeId),
    Unresolved(String),
    Ambiguous(Vec<String>),
}
