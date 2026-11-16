use nexora_core::NodeId;
pub(crate) struct ParseTask {
    pub start: usize,
    pub end: usize,
    pub parent: Option<NodeId>,
    pub context: Option<String>,
    pub region: Option<String>,
    pub depth: usize,
}
