use nexora_ast::AbstractSyntaxNode;
use nexora_core::{ExpressionKind as K, NodeId};
use nexora_language::GraphDefinition;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct ExpressionWriteSummary {
    pub identifiers: BTreeSet<NodeId>,
    pub uncertain: bool,
}
impl ExpressionWriteSummary {
    pub fn collect(
        nodes: &BTreeMap<&NodeId, &AbstractSyntaxNode>,
        graph: &GraphDefinition,
        remaining: &mut usize,
    ) -> Self {
        let mut summary = Self {
            identifiers: BTreeSet::new(),
            uncertain: false,
        };
        for node in nodes.values() {
            let Some(shape) = &node.expression else {
                continue;
            };
            let Some(rule) = graph.expression_bindings.get(&node.kind) else {
                continue;
            };
            if shape.kind != K::Assignment
                && !(matches!(shape.kind, K::Prefix | K::Postfix)
                    && shape
                        .operator
                        .as_ref()
                        .is_some_and(|op| rule.mutating_unary_operators.contains(op)))
            {
                continue;
            }
            if *remaining == 0 {
                summary.uncertain = true;
                break;
            }
            *remaining -= 1;
            let mut target = shape.operands.first().and_then(|id| nodes.get(id)).copied();
            let mut depth = 0;
            while target
                .and_then(|n| n.expression.as_ref())
                .is_some_and(|e| e.kind == K::Group)
            {
                if *remaining == 0 || depth >= 256 {
                    summary.uncertain = true;
                    break;
                }
                *remaining -= 1;
                depth += 1;
                target = target
                    .and_then(|n| n.expression.as_ref())
                    .and_then(|e| e.operands.first())
                    .and_then(|id| nodes.get(id))
                    .copied();
            }
            match target.and_then(|n| n.expression.as_ref().map(|e| (n, e.kind))) {
                Some((node, K::Identifier)) => {
                    summary.identifiers.insert(node.id.clone());
                }
                // A member write changes that member, not its receiver's lexical binding.
                Some((_, K::Member | K::Index)) => {}
                _ => summary.uncertain = true,
            }
        }
        summary
    }
}
