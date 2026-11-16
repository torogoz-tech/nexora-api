use crate::{FullTextSearch, GraphTraversal, SymbolSearch, TextMatch};
use nexora_core::NodeId;
use nexora_graph::{CodeNode, NodeKind};
use nexora_storage::Snapshot;
use std::collections::{HashSet, VecDeque};
pub struct RetrievalEngine<'a> {
    snapshot: &'a Snapshot,
}
impl<'a> RetrievalEngine<'a> {
    pub fn new(snapshot: &'a Snapshot) -> Self {
        Self { snapshot }
    }
}
impl SymbolSearch for RetrievalEngine<'_> {
    fn find_symbol(&self, name: &str, limit: usize) -> Vec<&CodeNode> {
        self.snapshot
            .graph
            .nodes
            .iter()
            .filter(|n| !matches!(n.kind, NodeKind::File | NodeKind::Repository))
            .filter(|n| n.name == name)
            .take(limit.min(1000))
            .collect()
    }
}
impl FullTextSearch for RetrievalEngine<'_> {
    fn search_code(&self, query: &str, limit: usize) -> Vec<TextMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        self.snapshot
            .sources
            .iter()
            .flat_map(|(path, source)| {
                source
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.contains(query))
                    .map(move |(i, line)| TextMatch {
                        path: path.clone(),
                        line: i + 1,
                        text: line.into(),
                    })
            })
            .take(limit.min(1000))
            .collect()
    }
}
impl GraphTraversal for RetrievalEngine<'_> {
    fn traverse(&self, start: &NodeId, depth: usize, limit: usize) -> Vec<&CodeNode> {
        let mut pending = VecDeque::from([(start.clone(), 0)]);
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        while let Some((id, level)) = pending.pop_front() {
            if result.len() >= limit.min(1000) {
                break;
            }
            if !visited.insert(id.clone()) {
                continue;
            }
            if let Some(node) = self.snapshot.graph.nodes.iter().find(|n| n.id == id) {
                result.push(node);
                if level < depth.min(32) {
                    for edge in self.snapshot.graph.edges.iter().filter(|e| e.source == id) {
                        if pending.len() < 10_000 {
                            pending.push_back((edge.target.clone(), level + 1));
                        }
                    }
                }
            }
        }
        result
    }
}
