use crate::{CodeEdge, CodeNode, EdgeKind, Evidence, NodeKind, ResolutionState};
use nexora_ast::AbstractSyntaxTree;
use nexora_core::{EdgeId, NexoraError, NodeId};
use nexora_language::LanguageDefinition;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeGraph {
    pub nodes: Vec<CodeNode>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub expression_policies:
        std::collections::BTreeMap<String, nexora_language::ExpressionBindingDefinition>,
    pub edges: Vec<CodeEdge>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub syntax_units: std::collections::BTreeMap<NodeId, crate::SyntaxUnitProvenance>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub unit_coverage: std::collections::BTreeMap<NodeId, nexora_core::SyntaxCoverage>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub syntax_coverage: std::collections::BTreeMap<String, nexora_core::SyntaxCoverage>,
}
impl CodeGraph {
    pub fn merge(&mut self, other: Self) {
        self.nodes.extend(other.nodes);
        self.expression_policies.extend(other.expression_policies);
        self.edges.extend(other.edges);
        self.syntax_coverage.extend(other.syntax_coverage);
        self.syntax_units.extend(other.syntax_units);
        self.unit_coverage.extend(other.unit_coverage);
    }

    pub fn from_ast(
        ast: &AbstractSyntaxTree,
        path: &str,
        language: &LanguageDefinition,
    ) -> Result<Self, NexoraError> {
        Ok(Self::project(ast, path, language, false)?.graph)
    }
    pub fn project(
        ast: &AbstractSyntaxTree,
        path: &str,
        language: &LanguageDefinition,
        complete: bool,
    ) -> Result<crate::GraphBuildResult, NexoraError> {
        let symbols = nexora_symbols::SymbolTable::from_ast(ast);
        Self::project_with_symbols(ast, &symbols, path, language, complete)
    }
    pub fn project_with_symbols(
        ast: &AbstractSyntaxTree,
        symbols: &nexora_symbols::SymbolTable,
        path: &str,
        language: &LanguageDefinition,
        complete: bool,
    ) -> Result<crate::GraphBuildResult, NexoraError> {
        let mut strategies = std::collections::BTreeMap::new();
        let file_id = NodeId(format!("file:{path}"));
        let mut policy_ids = std::collections::BTreeMap::new();
        let mut expression_policies = std::collections::BTreeMap::new();
        if language.graph.schema_version >= 10 {
            for (kind, policy) in &language.graph.expression_bindings {
                let id = crate::ExpressionPolicyRegistry::id(policy)?;
                policy_ids.insert(kind, id.clone());
                expression_policies.insert(id, policy.clone());
            }
        }
        let mut graph = Self {
            expression_policies,
            nodes: vec![CodeNode {
                expression_policy: None,
                mutation_operand: None,
                syntax_unit: None,
                id: file_id.clone(),
                kind: NodeKind::File,
                name: path.into(),
                expression: None,
                lexical_region: None,
                evidence: None,
                scope_id: None,
                owned_scope_id: ast
                    .hierarchical
                    .then(|| NodeId(format!("{path}:scope:file"))),
                scope_policy: ast.file_scope.clone(),
                regions: Vec::new(),
                captures: Default::default(),
            }],
            edges: Vec::new(),
            syntax_coverage: Default::default(),
            syntax_units: Default::default(),
            unit_coverage: Default::default(),
        };
        let mut coverage = nexora_core::SyntaxCoverage {
            hierarchical: ast.hierarchical,
            ..Default::default()
        };
        for node in &ast.nodes {
            for region in &node.regions {
                match region.status {
                    nexora_core::RegionStatus::Parsed => coverage.parsed_regions += 1,
                    nexora_core::RegionStatus::Partial => coverage.partial_regions += 1,
                    nexora_core::RegionStatus::Opaque => coverage.opaque_regions += 1,
                }
            }
            let owner = node
                .parent_id
                .as_ref()
                .map_or_else(|| file_id.clone(), |id| NodeId(format!("{path}:{}", id.0)));
            let suffix = node.id.0.strip_prefix("ast:").unwrap_or(&node.id.0);
            let kind = language
                .graph
                .mappings
                .get(&node.kind)
                .and_then(|k| NodeKind::parse(k))
                .ok_or_else(|| NexoraError::Configuration("unknown graph mapping".into()))?;
            let id = NodeId(format!("{path}:{}", node.id.0));
            let evidence = Evidence {
                syntax_unit: None,
                path: path.into(),
                span: node.span,
                ast_id: node.id.clone(),
                rule_version: language.manifest.version.clone(),
            };
            let mutation_operand = if language.graph.schema_version >= 10 {
                node.expression
                    .as_ref()
                    .filter(|shape| {
                        language
                            .graph
                            .expression_bindings
                            .get(&node.kind)
                            .is_some_and(|rule| {
                                shape.kind == nexora_core::ExpressionKind::Assignment
                                    || (matches!(
                                        shape.kind,
                                        nexora_core::ExpressionKind::Prefix
                                            | nexora_core::ExpressionKind::Postfix
                                    ) && shape.operator.as_ref().is_some_and(|op| {
                                        rule.mutating_unary_operators.contains(op)
                                    }))
                            })
                    })
                    .and_then(|shape| shape.operands.first())
                    .map(|id| NodeId(format!("{path}:{}", id.0)))
            } else {
                None
            };
            graph.nodes.push(CodeNode {
                expression_policy: if language.graph.schema_version >= 10
                    && node.expression.is_some()
                {
                    policy_ids.get(&node.kind).cloned()
                } else {
                    None
                },
                mutation_operand,
                syntax_unit: None,
                id: id.clone(),
                kind,
                name: node.name.clone(),
                lexical_region: node.lexical_region.as_ref().map(|r| {
                    let mut r = r.clone();
                    r.id = NodeId(format!("{path}:{}", r.id.0));
                    r.parent = r
                        .parent
                        .as_ref()
                        .map(|id| NodeId(format!("{path}:{}", id.0)));
                    r
                }),
                expression: node
                    .expression
                    .as_ref()
                    .map(|e| nexora_core::ExpressionShape {
                        kind: e.kind,
                        operator: e.operator.clone(),
                        operands: e
                            .operands
                            .iter()
                            .map(|id| NodeId(format!("{path}:{}", id.0)))
                            .collect(),
                    }),
                evidence: Some(evidence.clone()),
                scope_id: symbols
                    .node_scopes
                    .get(&node.id)
                    .map(|id| NodeId(format!("{path}:{}", id.0))),
                owned_scope_id: node.scope.as_ref().map(|_| {
                    NodeId(format!(
                        "{path}:{}",
                        nexora_symbols::SymbolTable::owned_scope(&node.id).0
                    ))
                }),
                scope_policy: node.scope.clone(),
                regions: node
                    .regions
                    .iter()
                    .cloned()
                    .map(|mut region| {
                        if let Some(proof) = &mut region.coverage {
                            proof.regions = proof
                                .regions
                                .iter()
                                .map(|id| NodeId(format!("{path}:{}", id.0)))
                                .collect();
                        }
                        region
                    })
                    .collect(),
                captures: node.captures.clone(),
            });
            if node.role == nexora_core::SyntaxRole::Reference {
                if let Some(strategy) = language.graph.symbol_references.get(&node.kind) {
                    strategies.insert(id.clone(), strategy.clone());
                }
                let relations = language
                    .graph
                    .reference_relations
                    .get(&node.kind)
                    .ok_or_else(|| {
                        NexoraError::Configuration("missing AST reference mapping".into())
                    })?;
                for relation in relations {
                    let kind = match relation.as_str() {
                        "IMPORTS" => EdgeKind::Imports,
                        "DEPENDS_ON" => EdgeKind::DependsOn,
                        "EXPORTS" => EdgeKind::Exports,
                        "REFERENCES" => EdgeKind::References,
                        _ => {
                            return Err(NexoraError::Configuration(
                                "unsupported reference mapping".into(),
                            ));
                        }
                    };
                    graph.edges.push(CodeEdge {
                        call_binding: None,
                        scope_binding: None,
                        resolution_reason: None,
                        id: EdgeId(format!("{path}:{relation}:{suffix}")),
                        source: if matches!(kind, EdgeKind::Imports | EdgeKind::DependsOn) {
                            file_id.clone()
                        } else {
                            owner.clone()
                        },
                        target: id.clone(),
                        kind,
                        evidence: evidence.clone(),
                        resolution: ResolutionState::Unresolved,
                        label: language
                            .graph
                            .symbol_references
                            .get(&node.kind)
                            .and_then(|s| s.label.clone()),
                    });
                }
                continue;
            }
            graph.edges.push(CodeEdge {
                call_binding: None,
                scope_binding: None,
                resolution_reason: None,
                id: EdgeId(format!(
                    "{path}:{}:{suffix}",
                    if matches!(
                        node.role,
                        nexora_core::SyntaxRole::Container | nexora_core::SyntaxRole::Expression
                    ) {
                        "contains"
                    } else {
                        "declares"
                    }
                )),
                source: owner,
                target: id,
                kind: if matches!(
                    node.role,
                    nexora_core::SyntaxRole::Container | nexora_core::SyntaxRole::Expression
                ) {
                    EdgeKind::Contains
                } else {
                    EdgeKind::Declares
                },
                evidence,
                resolution: ResolutionState::Resolved,
                label: None,
            });
        }
        let mut diagnostics = crate::local_symbol_resolver::LocalSymbolResolver::resolve(
            &mut graph,
            &strategies,
            complete && ast.complete && symbols.complete,
            symbols,
        );
        diagnostics.extend(
            crate::expression_binding_resolver::ExpressionBindingResolver::resolve(
                &mut graph,
                ast,
                symbols,
                path,
                language,
                complete && ast.complete && symbols.complete,
            ),
        );
        graph.syntax_coverage.insert(path.into(), coverage);
        Ok(crate::GraphBuildResult { graph, diagnostics })
    }
}
