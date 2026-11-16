use crate::{RegionInterpretation, SyntaxUnit, unit_graph_namespace::UnitGraphNamespace};
use nexora_core::{EdgeId, NexoraError, NodeId, RegionStatus, SyntaxCoverage};
use nexora_graph::{
    CodeEdge, CodeGraph, CodeNode, EdgeKind, NodeKind, ResolutionState, SyntaxUnitProvenance,
};
use std::collections::BTreeMap;
pub(crate) struct CompositionGraphProjector;
impl CompositionGraphProjector {
    pub fn project(
        path: &str,
        units: &[SyntaxUnit],
        links: &[RegionInterpretation],
    ) -> Result<CodeGraph, NexoraError> {
        let mut graph = CodeGraph::default();
        let namespaces: BTreeMap<_, _> = units
            .iter()
            .enumerate()
            .map(|(i, u)| (u.id.clone(), UnitGraphNamespace::new(path, &u.id, i == 0)))
            .collect();
        let mut aggregate = SyntaxCoverage {
            hierarchical: true,
            ..Default::default()
        };
        for (index, unit) in units.iter().enumerate() {
            graph
                .expression_policies
                .extend(unit.graph.expression_policies.clone());
            let namespace = &namespaces[&unit.id];
            let owner = namespace.owner();
            graph.syntax_units.insert(
                owner.clone(),
                SyntaxUnitProvenance {
                    direct_calls: false,
                    mutation_analysis: unit.mutation_analysis,
                    unit_id: unit.id.clone(),
                    path: path.into(),
                    language: unit.language.clone(),
                    rule_version: unit.rule_version.clone(),
                    profile_fingerprint: unit.profile_fingerprint.clone(),
                    context: unit.context.clone(),
                    lexical_context: unit.lexical_context.clone(),
                    span: unit.span,
                    status: unit.status,
                },
            );
            let mut has_root = false;
            for original in &unit.graph.nodes {
                let mut node = original.clone();
                node.id = namespace.node(&original.id);
                node.syntax_unit = Some(owner.clone());
                node.mutation_operand = node.mutation_operand.as_ref().map(|id| namespace.node(id));
                if original.kind == NodeKind::File {
                    has_root = true;
                    if index != 0 {
                        node.kind = NodeKind::SyntaxUnit;
                    }
                }
                if let Some(evidence) = &mut node.evidence {
                    evidence.syntax_unit = Some(owner.clone());
                }
                node.scope_id = node.scope_id.as_ref().map(|id| namespace.scope(id));
                node.owned_scope_id = node.owned_scope_id.as_ref().map(|id| namespace.scope(id));
                if let Some(expression) = &mut node.expression {
                    expression.operands = expression
                        .operands
                        .iter()
                        .map(|id| namespace.node(id))
                        .collect();
                }
                if let Some(region) = &mut node.lexical_region {
                    region.id = namespace.node(&region.id);
                    region.parent = region.parent.as_ref().map(|id| namespace.node(id));
                }
                for region in &mut node.regions {
                    if let Some(proof) = &mut region.coverage {
                        proof.regions = proof.regions.iter().map(|id| namespace.node(id)).collect();
                        proof.syntax_units = proof
                            .syntax_units
                            .iter()
                            .map(|id| {
                                namespaces.get(&id.0).map(|ns| ns.owner()).ok_or_else(|| {
                                    NexoraError::Configuration(
                                        "unknown coverage interpretation".into(),
                                    )
                                })
                            })
                            .collect::<Result<_, _>>()?;
                    }
                }
                graph.nodes.push(node);
            }
            // A lexical failure has no fabricated syntax graph. Its structural unit
            // still exists, with metadata/diagnostics and no invented AST evidence.
            if !has_root {
                graph.nodes.push(CodeNode {
                    expression_policy: None,
                    mutation_operand: None,
                    id: owner.clone(),
                    kind: if index == 0 {
                        NodeKind::File
                    } else {
                        NodeKind::SyntaxUnit
                    },
                    name: path.into(),
                    syntax_unit: Some(owner.clone()),
                    lexical_region: None,
                    expression: None,
                    evidence: None,
                    scope_id: None,
                    owned_scope_id: Some(namespace.scope(&NodeId(format!("{path}:scope:file")))),
                    scope_policy: unit.cst.file_scope.clone(),
                    regions: Vec::new(),
                    captures: Default::default(),
                });
            }
            for original in &unit.graph.edges {
                let mut edge = original.clone();
                edge.id = namespace.edge(&original.id);
                edge.source = namespace.node(&original.source);
                edge.target = namespace.node(&original.target);
                edge.evidence.syntax_unit = Some(owner.clone());
                graph.edges.push(edge);
            }
            let mut coverage = unit
                .graph
                .syntax_coverage
                .get(path)
                .cloned()
                .unwrap_or_else(|| SyntaxCoverage {
                    hierarchical: unit.cst.hierarchical,
                    partial_regions: usize::from(unit.status == RegionStatus::Partial),
                    ..Default::default()
                });
            if unit.status == RegionStatus::Partial && coverage.partial_regions == 0 {
                coverage.partial_regions = 1;
            }
            aggregate.hierarchical &= coverage.hierarchical;
            aggregate.parsed_regions += coverage.parsed_regions;
            aggregate.opaque_regions += coverage.opaque_regions;
            aggregate.partial_regions += coverage.partial_regions;
            graph.unit_coverage.insert(owner, coverage);
        }
        let node_indexes: BTreeMap<_, _> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        for (index, link) in links.iter().enumerate() {
            let source_namespace = namespaces
                .get(&link.source_unit)
                .ok_or_else(|| NexoraError::Configuration("unknown source syntax unit".into()))?;
            let target_namespace = namespaces
                .get(&link.target_unit)
                .ok_or_else(|| NexoraError::Configuration("unknown target syntax unit".into()))?;
            let source = source_namespace.node(&link.region_id);
            let node = node_indexes
                .get(&source)
                .and_then(|i| graph.nodes.get(*i))
                .ok_or_else(|| {
                    NexoraError::Configuration("interpretation has no region graph endpoint".into())
                })?;
            let evidence = node
                .evidence
                .clone()
                .filter(|_| node.kind == NodeKind::LexicalRegion)
                .ok_or_else(|| {
                    NexoraError::Configuration(
                        "interpretation endpoint has no lexical evidence".into(),
                    )
                })?;
            graph.edges.push(CodeEdge {
                call_binding: None,
                scope_binding: None,
                id: EdgeId(format!("interpretation:{path}:{index}")),
                source,
                target: target_namespace.owner(),
                kind: EdgeKind::Interprets,
                evidence,
                resolution: ResolutionState::Resolved,
                resolution_reason: Some(
                    graph
                        .syntax_units
                        .get(&target_namespace.owner())
                        .and_then(|unit| unit.lexical_context.as_ref())
                        .map_or_else(
                            || "declared_embedded_context".into(),
                            |name| format!("declared_embedded_context/lexical:{name}"),
                        ),
                ),
                label: Some(link.component.name().into()),
            });
        }
        for (index, mut bridge) in nexora_graph::ScopeBridgeOrigins::select(&graph)
            .into_iter()
            .enumerate()
        {
            bridge.id = EdgeId(format!("scope-bridge:{path}:{index}"));
            bridge.kind = EdgeKind::ScopeBridge;
            bridge.resolution_reason = Some("declared_scope_bridge/v1".into());
            graph.edges.push(bridge);
        }
        nexora_graph::GraphIntegrityValidator::validate(&graph)?;
        graph.syntax_coverage.insert(path.into(), aggregate);
        Ok(graph)
    }
}
