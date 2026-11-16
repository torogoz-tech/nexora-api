use crate::{CodeGraph, CodeNode, EdgeKind, SyntaxUnitIdentity};
use nexora_core::NexoraError;
use nexora_core::NodeId;
use std::collections::BTreeMap;
pub(crate) struct LexicalEntryProvenanceValidator;
impl LexicalEntryProvenanceValidator {
    pub fn validate(
        graph: &CodeGraph,
        nodes: &BTreeMap<&NodeId, &CodeNode>,
    ) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("inconsistent lexical-entry provenance".into());
        for (owner, unit) in &graph.syntax_units {
            if unit.lexical_context.as_ref().is_some_and(|name| {
                name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            }) {
                return Err(fail());
            }
            if unit.lexical_context.is_some() {
                if *owner != SyntaxUnitIdentity::graph_owner(&unit.path, &unit.unit_id) {
                    return Err(fail());
                }
                let scope = nodes
                    .get(owner)
                    .and_then(|node| node.scope_policy.as_ref())
                    .ok_or_else(fail)?;
                let context = unit.context.as_deref().ok_or_else(fail)?;
                if unit.unit_id
                    != SyntaxUnitIdentity::for_interpretation(
                        unit.span,
                        &unit.language.0,
                        context,
                        unit.lexical_context.as_deref(),
                        scope,
                        &unit.profile_fingerprint,
                    )?
                {
                    return Err(fail());
                }
            } else if unit.unit_id.starts_with("unit:lex:")
                || owner
                    .0
                    .starts_with(&SyntaxUnitIdentity::graph_owner(&unit.path, "unit:lex:").0)
            {
                return Err(fail());
            }
        }
        for edge in graph
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Interprets)
        {
            let target = graph.syntax_units.get(&edge.target).ok_or_else(fail)?;
            match &target.lexical_context {
                Some(name)
                    if edge.resolution_reason.as_deref()
                        != Some(format!("declared_embedded_context/lexical:{name}").as_str()) =>
                {
                    return Err(fail());
                }
                None if edge.resolution_reason.as_deref().is_some_and(|reason| {
                    reason.starts_with("declared_embedded_context/lexical:")
                }) =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
        Ok(())
    }
}
