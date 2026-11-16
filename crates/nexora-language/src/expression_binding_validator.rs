use crate::{GrammarDefinition, GraphDefinition};
use nexora_core::{NexoraError, SyntaxRole};
use std::collections::BTreeSet;
pub struct ExpressionBindingValidator;
impl ExpressionBindingValidator {
    pub fn validate(
        graph: &GraphDefinition,
        grammar: &GrammarDefinition,
    ) -> Result<(), NexoraError> {
        let fail =
            || NexoraError::Configuration("invalid expression binding strategy or schema".into());
        if graph.expression_bindings.len() > 64
            || (!graph.expression_bindings.is_empty()
                && (![5, 6, 7, 8, 9, 10].contains(&graph.schema_version)
                    || ![4, 5, 6, 7, 8, 9, 10].contains(&grammar.schema_version)))
        {
            return Err(fail());
        }
        if graph
            .expression_bindings
            .values()
            .any(|rule| !rule.callable_kinds.is_empty())
            && grammar
                .contexts
                .values()
                .filter_map(|c| c.expression.as_ref())
                .any(|e| !graph.expression_bindings.contains_key(&e.ast_kind))
        {
            return Err(fail());
        }
        let declarations: BTreeSet<_> = grammar
            .productions
            .iter()
            .filter(|p| p.projection.role == SyntaxRole::Declaration)
            .filter_map(|p| graph.mappings.get(&p.projection.ast_kind))
            .collect();
        for (kind, rule) in &graph.expression_bindings {
            let contexts: Vec<_> = grammar
                .contexts
                .values()
                .filter_map(|c| c.expression.as_ref())
                .filter(|e| &e.ast_kind == kind)
                .collect();
            if contexts.is_empty()
                || graph.mappings.get(kind).is_none_or(|k| k != "Expression")
                || rule.target_kinds.is_empty()
                || rule.target_kinds.len() > 16
                || rule.callable_kinds.len() > 16
                || rule.mutating_unary_operators.len() > 128
            {
                return Err(fail());
            }
            for list in [
                &rule.target_kinds,
                &rule.callable_kinds,
                &rule.mutating_unary_operators,
            ] {
                if list.iter().collect::<BTreeSet<_>>().len() != list.len() {
                    return Err(fail());
                }
            }
            if rule.target_kinds.iter().any(|k| !declarations.contains(k))
                || rule
                    .callable_kinds
                    .iter()
                    .any(|k| !rule.target_kinds.contains(k))
                || rule.mutating_unary_operators.iter().any(|op| {
                    !contexts.iter().all(|e| {
                        e.prefix
                            .iter()
                            .chain(e.postfix.iter())
                            .any(|u| &u.token.text == op)
                    })
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
