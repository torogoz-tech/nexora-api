use crate::{CodeGraph, NodeKind};
use nexora_core::{ExpressionKind as K, NexoraError};
use std::collections::HashMap;
/// Source expression occurrences form a forest; semantic graph edges may be cyclic.
pub(crate) struct ExpressionShapeValidator;
impl ExpressionShapeValidator {
    pub fn validate(graph: &CodeGraph) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("invalid expression shape forest".into());
        let indices: HashMap<_, _> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (&n.id, i))
            .collect();
        if indices.len() != graph.nodes.len() {
            return Err(fail());
        }
        let mut owned = vec![None; graph.nodes.len()];
        let mut count = 0usize;
        for (parent_index, node) in graph.nodes.iter().enumerate() {
            let Some(shape) = &node.expression else {
                continue;
            };
            count += 1;
            let length = shape.operands.len();
            let arity = match shape.kind {
                K::Identifier | K::Literal => length == 0,
                K::Prefix | K::Postfix | K::Group => length == 1,
                K::Infix | K::Assignment | K::Member => length == 2,
                K::Application => (1..=1025).contains(&length),
                K::Index => (2..=1025).contains(&length),
                K::Conditional => length == 3,
            };
            let leaf = matches!(shape.kind, K::Identifier | K::Literal);
            if node.kind != NodeKind::Expression
                || !arity
                || (leaf && shape.operator.is_some())
                || (!leaf && !shape.operator.as_ref().is_some_and(|s| !s.is_empty()))
            {
                return Err(fail());
            }
            for operand in &shape.operands {
                let index = *indices.get(operand).ok_or_else(fail)?;
                let child = &graph.nodes[index];
                if owned[index].is_some()
                    || child.expression.is_none()
                    || child.syntax_unit != node.syntax_unit
                {
                    return Err(fail());
                }
                owned[index] = Some(parent_index);
                if let (Some(parent), Some(child)) = (&node.evidence, &child.evidence) {
                    let p = parent.span;
                    let c = child.span;
                    if parent.path != child.path
                        || c.start.offset < p.start.offset
                        || c.end.offset > p.end.offset
                        || c.end
                            .offset
                            .checked_sub(c.start.offset)
                            .zip(p.end.offset.checked_sub(p.start.offset))
                            .is_none_or(|(c, p)| c >= p)
                    {
                        return Err(fail());
                    }
                }
            }
            if shape.kind == K::Member
                && graph.nodes[indices[&shape.operands[1]]]
                    .expression
                    .as_ref()
                    .is_none_or(|s| s.kind != K::Identifier)
            {
                return Err(fail());
            }
        }
        let mut containment = vec![None; graph.nodes.len()];
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.kind == crate::EdgeKind::Contains)
        {
            let target = *indices.get(&edge.target).ok_or_else(fail)?;
            if graph.nodes[target].expression.is_none() {
                continue;
            }
            let source = *indices.get(&edge.source).ok_or_else(fail)?;
            if containment[target].replace(source).is_some()
                || graph.nodes[source].syntax_unit != graph.nodes[target].syntax_unit
                || (graph.nodes[source].expression.is_some() && owned[target] != Some(source))
                || owned[target].is_some_and(|owner| owner != source)
            {
                return Err(fail());
            }
        }
        if owned
            .iter()
            .enumerate()
            .any(|(index, owner)| owner.is_some() && *owner != containment[index])
        {
            return Err(fail());
        }
        let mut pending: Vec<_> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| n.expression.is_some() && owned[*i].is_none())
            .map(|(i, _)| i)
            .collect();
        let mut visited = 0usize;
        while let Some(index) = pending.pop() {
            visited += 1;
            for operand in &graph.nodes[index]
                .expression
                .as_ref()
                .expect("expression node")
                .operands
            {
                pending.push(*indices.get(operand).ok_or_else(fail)?);
            }
        }
        if visited != count {
            return Err(fail());
        }
        Ok(())
    }
}
