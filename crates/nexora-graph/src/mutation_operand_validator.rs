use crate::CodeGraph;
use nexora_core::{ExpressionKind as K, NexoraError};
use std::collections::BTreeMap;
pub(crate) struct MutationOperandValidator;
impl MutationOperandValidator {
    pub fn validate(graph: &CodeGraph) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("invalid declared mutation operand".into());
        for (id, policy) in &graph.expression_policies {
            if crate::ExpressionPolicyRegistry::id(policy)? != *id {
                return Err(fail());
            }
        }
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        for node in &graph.nodes {
            let policy = node
                .expression_policy
                .as_ref()
                .map(|id| graph.expression_policies.get(id).ok_or_else(fail))
                .transpose()?;
            let complete = node
                .syntax_unit
                .as_ref()
                .and_then(|id| graph.syntax_units.get(id))
                .is_some_and(|u| u.mutation_analysis);
            if complete && node.expression.is_some() && policy.is_none() {
                return Err(fail());
            }
            if let Some(policy) = policy {
                if node.expression.is_none()
                    || policy.target_kinds.is_empty()
                    || policy.target_kinds.len() > 16
                    || policy.callable_kinds.len() > 16
                    || policy.mutating_unary_operators.len() > 128
                    || policy
                        .target_kinds
                        .iter()
                        .any(|k| crate::NodeKind::parse(k).is_none())
                    || policy
                        .callable_kinds
                        .iter()
                        .any(|k| !policy.target_kinds.contains(k))
                {
                    return Err(fail());
                }
                let mutates = node.expression.as_ref().is_some_and(|s| {
                    s.kind == K::Assignment
                        || (matches!(s.kind, K::Prefix | K::Postfix)
                            && s.operator
                                .as_ref()
                                .is_some_and(|op| policy.mutating_unary_operators.contains(op)))
                });
                if mutates != node.mutation_operand.is_some() {
                    return Err(fail());
                }
            }
            if let Some(target) = &node.mutation_operand {
                let shape = node.expression.as_ref().ok_or_else(fail)?;
                if !matches!(shape.kind, K::Assignment | K::Prefix | K::Postfix)
                    || shape.operands.first() != Some(target)
                    || nodes
                        .get(target)
                        .is_none_or(|target| target.syntax_unit != node.syntax_unit)
                {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}
