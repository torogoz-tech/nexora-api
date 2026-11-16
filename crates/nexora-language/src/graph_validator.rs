use crate::{GrammarDefinition, GraphDefinition};
use nexora_core::{NexoraError, SyntaxRole};
use std::collections::BTreeSet;
pub struct GraphValidator;
impl GraphValidator {
    pub fn validate(
        graph: &GraphDefinition,
        grammar: &GrammarDefinition,
    ) -> Result<(), NexoraError> {
        let fail = || {
            NexoraError::Configuration("invalid graph mappings or symbol reference strategy".into())
        };
        if ![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].contains(&graph.schema_version)
            || graph.mappings.len() > 128
            || graph.reference_relations.len() > 128
            || graph.symbol_references.len() > 128
        {
            return Err(fail());
        }
        let canonical = [
            "LexicalRegion",
            "TypeAlias",
            "Enum",
            "Repository",
            "File",
            "Module",
            "Namespace",
            "Class",
            "Interface",
            "Function",
            "Method",
            "Variable",
            "Parameter",
            "Symbol",
            "Block",
            "Expression",
            "Statement",
        ];
        if graph.mappings.values().any(|k| {
            !canonical.contains(&k.as_str())
                || (k == "LexicalRegion" && graph.schema_version < 7)
                || (["TypeAlias", "Enum"].contains(&k.as_str()) && graph.schema_version < 6)
                || (k == "Symbol" && graph.schema_version < 2)
                || (k == "Block" && graph.schema_version < 3)
                || (["Expression", "Statement"].contains(&k.as_str()) && graph.schema_version < 4)
        }) {
            return Err(fail());
        }
        crate::expression_binding_validator::ExpressionBindingValidator::validate(graph, grammar)?;
        if grammar.schema_version >= 3
            && graph.schema_version != grammar.schema_version
            && !([4, 5, 6, 7, 8, 9, 10].contains(&grammar.schema_version)
                && [4, 5, 6, 7, 8, 9, 10].contains(&graph.schema_version))
        {
            return Err(fail());
        }
        let references: BTreeSet<_> = grammar
            .productions
            .iter()
            .filter(|p| p.projection.role == SyntaxRole::Reference)
            .map(|p| p.projection.ast_kind.as_str())
            .collect();
        for production in &grammar.productions {
            if graph
                .mappings
                .get(&production.projection.ast_kind)
                .is_some_and(|k| k == "LexicalRegion")
            {
                return Err(fail());
            }
            if production.projection.role == SyntaxRole::Container
                && (graph.schema_version < 3
                    || graph
                        .mappings
                        .get(&production.projection.ast_kind)
                        .is_none_or(|k| {
                            k != "Block" && (graph.schema_version < 4 || k != "Statement")
                        }))
            {
                return Err(fail());
            }
            if production.projection.role != SyntaxRole::Reference
                && graph
                    .mappings
                    .get(&production.projection.ast_kind)
                    .is_some_and(|k| k == "Symbol")
            {
                return Err(fail());
            }
        }
        for (kind, relations) in &graph.reference_relations {
            if !references.contains(kind.as_str())
                || relations.is_empty()
                || relations.len() > 2
                || relations.iter().collect::<BTreeSet<_>>().len() != relations.len()
            {
                return Err(fail());
            }
            match graph.mappings.get(kind).map(String::as_str) {
                Some("Module")
                    if !graph.symbol_references.contains_key(kind)
                        && relations
                            .iter()
                            .all(|r| ["IMPORTS", "DEPENDS_ON"].contains(&r.as_str())) => {}
                Some("Symbol")
                    if graph.schema_version >= 2
                        && graph.symbol_references.contains_key(kind)
                        && relations
                            .iter()
                            .all(|r| ["EXPORTS", "REFERENCES"].contains(&r.as_str())) => {}
                _ => return Err(fail()),
            }
        }
        for (kind, strategy) in &graph.symbol_references {
            if graph.schema_version < 2
                || (matches!(strategy.scope, crate::SymbolScope::Lexical)
                    && (graph.schema_version < 3 || grammar.schema_version < 3))
                || !references.contains(kind.as_str())
                || graph.mappings.get(kind).is_none_or(|k| k != "Symbol")
                || !graph.reference_relations.contains_key(kind)
                || strategy.target_kinds.is_empty()
                || strategy.target_kinds.len() > 8
                || strategy.target_kinds.iter().collect::<BTreeSet<_>>().len()
                    != strategy.target_kinds.len()
                || strategy.target_kinds.iter().any(|k| {
                    !canonical.contains(&k.as_str())
                        || (["TypeAlias", "Enum"].contains(&k.as_str()) && graph.schema_version < 6)
                        || ["Repository", "File", "Symbol", "LexicalRegion"].contains(&k.as_str())
                })
                || strategy.label.as_ref().is_some_and(|l| {
                    l.is_empty() || l.len() > 128 || l.chars().any(char::is_control)
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
