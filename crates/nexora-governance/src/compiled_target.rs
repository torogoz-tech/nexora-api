use nexora_graph::NodeKind;
use regex::Regex;
#[derive(Debug, Clone)]
pub struct CompiledTarget {
    pub target: crate::PolicyTarget,
    pub name_pattern: Option<Regex>,
    pub file_pattern: Option<Regex>,
    pub kind: Option<NodeKind>,
}
