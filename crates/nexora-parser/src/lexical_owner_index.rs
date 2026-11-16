use crate::ConcreteSyntaxNode;
use nexora_core::{NodeId, SourceSpan};
pub(crate) struct LexicalOwnerIndex {
    owners: Vec<(SourceSpan, NodeId)>,
    cursor: usize,
    active: Vec<(SourceSpan, NodeId)>,
}
impl LexicalOwnerIndex {
    pub fn new(nodes: &[ConcreteSyntaxNode]) -> Self {
        let mut owners: Vec<_> = nodes.iter().map(|n| (n.span, n.id.clone())).collect();
        owners.sort_by_key(|(s, id)| (s.start.offset, std::cmp::Reverse(s.end.offset), id.clone()));
        Self {
            owners,
            cursor: 0,
            active: Vec::new(),
        }
    }
    pub fn owner(&mut self, span: SourceSpan) -> Option<NodeId> {
        while self.cursor < self.owners.len()
            && self.owners[self.cursor].0.start.offset <= span.start.offset
        {
            let current = &self.owners[self.cursor];
            while self.active.last().is_some_and(|(s, _)| {
                s.end.offset <= current.0.start.offset || s.end.offset < current.0.end.offset
            }) {
                self.active.pop();
            }
            self.active.push(current.clone());
            self.cursor += 1;
        }
        while self
            .active
            .last()
            .is_some_and(|(s, _)| s.end.offset < span.end.offset)
        {
            self.active.pop();
        }
        self.active.last().map(|(_, id)| id.clone())
    }
}
