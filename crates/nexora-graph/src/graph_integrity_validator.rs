use crate::{CodeGraph, Evidence, NodeKind};
use nexora_core::NexoraError;
use std::collections::{BTreeMap, BTreeSet};
pub struct GraphIntegrityValidator;
impl GraphIntegrityValidator {
    pub fn validate(graph: &CodeGraph) -> Result<(), NexoraError> {
        let fail = || {
            NexoraError::Configuration(
                "graph has duplicate IDs, missing endpoints or invalid syntax-unit provenance"
                    .into(),
            )
        };
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let edges: BTreeSet<_> = graph.edges.iter().map(|e| &e.id).collect();
        if nodes.len() != graph.nodes.len()
            || edges.len() != graph.edges.len()
            || graph
                .edges
                .iter()
                .any(|e| !nodes.contains_key(&e.source) || !nodes.contains_key(&e.target))
        {
            return Err(fail());
        }
        if graph.syntax_units.len() > graph.nodes.len()
            || graph.unit_coverage.len() != graph.syntax_units.len()
            || graph
                .unit_coverage
                .keys()
                .any(|id| !graph.syntax_units.contains_key(id))
        {
            return Err(fail());
        }
        for (id, origin) in &graph.syntax_units {
            let node = nodes.get(id).ok_or_else(fail)?;
            if !matches!(node.kind, NodeKind::File | NodeKind::SyntaxUnit)
                || node.syntax_unit.as_ref() != Some(id)
                || origin.span.start.offset > origin.span.end.offset
            {
                return Err(fail());
            }
        }
        for node in &graph.nodes {
            if !graph.syntax_units.is_empty()
                && node.kind != NodeKind::Repository
                && node.syntax_unit.is_none()
            {
                return Err(fail());
            }
            if node
                .syntax_unit
                .as_ref()
                .is_some_and(|id| !graph.syntax_units.contains_key(id))
                || (node.kind == NodeKind::SyntaxUnit && !graph.syntax_units.contains_key(&node.id))
            {
                return Err(fail());
            }
            if let Some(evidence) = &node.evidence {
                if evidence.syntax_unit != node.syntax_unit {
                    return Err(fail());
                }
                Self::evidence(graph, evidence)?;
            }
        }
        for edge in &graph.edges {
            let source_owner = &nodes[&edge.source].syntax_unit;
            let target_owner = &nodes[&edge.target].syntax_unit;
            if source_owner.is_some()
                && (source_owner != &edge.evidence.syntax_unit
                    || (!matches!(
                        edge.kind,
                        crate::EdgeKind::Interprets | crate::EdgeKind::ScopeBridge
                    ) && source_owner != target_owner
                        && edge.scope_binding.is_none()
                        && edge.call_binding.is_none()
                        && !Self::module_reference(
                            graph,
                            edge,
                            nodes[&edge.source],
                            nodes[&edge.target],
                        )))
            {
                return Err(fail());
            }
            Self::evidence(graph, &edge.evidence)?;
            if edge.kind == crate::EdgeKind::Interprets {
                let source = nodes[&edge.source];
                let target = graph.syntax_units.get(&edge.target).ok_or_else(fail)?;
                let source_region = source.lexical_region.as_ref().ok_or_else(fail)?;
                let source_evidence = source.evidence.as_ref().ok_or_else(fail)?;
                let component = match edge.label.as_deref() {
                    None | Some("content") => nexora_core::LexicalRegionComponent::Content,
                    Some("header") => nexora_core::LexicalRegionComponent::Header,
                    _ => return Err(fail()),
                };
                if source.kind != NodeKind::LexicalRegion
                    || !graph.syntax_units.contains_key(&edge.target)
                    || nodes[&edge.target].kind != NodeKind::SyntaxUnit
                    || source.syntax_unit.as_ref() == Some(&edge.target)
                    || source.syntax_unit != edge.evidence.syntax_unit
                    || component.input(source_region) != Some(target.span)
                    || source_evidence.ast_id != edge.evidence.ast_id
                    || source_evidence.span != edge.evidence.span
                    || source_evidence.path != target.path
                {
                    return Err(fail());
                }
            }
        }
        crate::lexical_entry_provenance_validator::LexicalEntryProvenanceValidator::validate(
            graph, &nodes,
        )?;
        crate::expression_shape_validator::ExpressionShapeValidator::validate(graph)?;
        crate::mutation_operand_validator::MutationOperandValidator::validate(graph)?;
        crate::scope_bridge_validator::ScopeBridgeValidator::validate(graph)?;
        crate::scope_binding_validator::ScopeBindingValidator::validate(graph)?;
        crate::call_binding_validator::CallBindingValidator::validate(graph)?;
        crate::region_coverage_validator::RegionCoverageValidator::validate(graph, &nodes)?;
        Ok(())
    }
    fn module_reference(
        graph: &CodeGraph,
        edge: &crate::CodeEdge,
        source: &crate::CodeNode,
        target: &crate::CodeNode,
    ) -> bool {
        matches!(
            edge.kind,
            crate::EdgeKind::Imports | crate::EdgeKind::DependsOn
        ) && edge.resolution == crate::ResolutionState::Resolved
            && edge.resolution_reason.as_deref() == Some("declared_module_resolution/v1")
            && matches!(source.kind, NodeKind::File | NodeKind::SyntaxUnit)
            && target.kind == NodeKind::File
            && source
                .syntax_unit
                .as_ref()
                .is_some_and(|id| graph.syntax_units.contains_key(id))
            && target.syntax_unit.as_ref() == Some(&target.id)
            && graph
                .syntax_units
                .get(&target.id)
                .is_some_and(|unit| unit.path == target.name && unit.context.is_none())
    }
    fn evidence(graph: &CodeGraph, evidence: &Evidence) -> Result<(), NexoraError> {
        if let Some(id) = &evidence.syntax_unit {
            let origin = graph
                .syntax_units
                .get(id)
                .ok_or_else(|| NexoraError::Configuration("unknown evidence syntax unit".into()))?;
            if evidence.path != origin.path
                || evidence.rule_version != origin.rule_version
                || evidence.span.start.offset < origin.span.start.offset
                || evidence.span.end.offset > origin.span.end.offset
                || evidence.span.start.offset > evidence.span.end.offset
            {
                return Err(NexoraError::Configuration(
                    "evidence contradicts its syntax-unit provenance".into(),
                ));
            }
        }
        Ok(())
    }
}
