use nexora_graph::EdgeKind;
#[derive(Debug, Clone)]
pub struct CompiledRelation {
    pub kind: EdgeKind,
    pub target: crate::PolicyRelationTarget,
    pub traversal: crate::TraversalLimit,
    pub target_file_pattern: Option<regex::Regex>,
}
