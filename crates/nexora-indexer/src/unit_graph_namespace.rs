use nexora_core::{EdgeId, NodeId};
pub(crate) struct UnitGraphNamespace {
    path: String,
    unit: String,
    root: bool,
}
impl UnitGraphNamespace {
    pub fn new(path: &str, unit: &str, root: bool) -> Self {
        Self {
            path: path.into(),
            unit: unit.into(),
            root,
        }
    }
    pub fn owner(&self) -> NodeId {
        if self.root {
            NodeId(format!("file:{}", self.path))
        } else {
            nexora_graph::SyntaxUnitIdentity::graph_owner(&self.path, &self.unit)
        }
    }
    pub fn node(&self, id: &NodeId) -> NodeId {
        if id.0 == format!("file:{}", self.path) {
            self.owner()
        } else {
            NodeId(format!(
                "unit-node:{}:{}:{}",
                self.unit.len(),
                self.unit,
                id.0
            ))
        }
    }
    pub fn scope(&self, id: &NodeId) -> NodeId {
        NodeId(format!(
            "unit-scope:{}:{}:{}",
            self.unit.len(),
            self.unit,
            id.0
        ))
    }
    pub fn edge(&self, id: &EdgeId) -> EdgeId {
        EdgeId(format!(
            "unit-edge:{}:{}:{}",
            self.unit.len(),
            self.unit,
            id.0
        ))
    }
}
