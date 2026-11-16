use crate::{GovernanceError, NodeAnchor, PolicyScope};
use nexora_core::{RepositoryId, SourcePosition, SourceSpan, TenantId};
use nexora_graph::{CodeNode, EdgeKind, NodeKind};
use nexora_storage::Snapshot;
pub struct EvaluationContext<'a> {
    pub snapshot: &'a Snapshot,
    pub language: &'a str,
    pub organization: Option<&'a str>,
    pub coverage: crate::AnalysisCoverage,
    node_index: std::collections::BTreeMap<nexora_core::NodeId, usize>,
    module_coverage: std::collections::BTreeSet<(String, String, String)>,
}
impl<'a> EvaluationContext<'a> {
    pub fn new(
        snapshot: &'a Snapshot,
        tenant: &TenantId,
        repository: &RepositoryId,
        language: &'a str,
        organization: Option<&'a str>,
    ) -> Result<Self, GovernanceError> {
        if &snapshot.tenant_id != tenant || &snapshot.repository_id != repository {
            return Err(GovernanceError::Scope(
                "snapshot does not match authorized tenant/repository".into(),
            ));
        }
        if snapshot.sources.len() > 10_000
            || snapshot.sources.values().any(|s| s.len() > 1_048_576)
            || snapshot.graph.nodes.len() > 10_000
            || snapshot.graph.edges.len() > 50_000
            || snapshot.sources.values().map(|s| s.len()).sum::<usize>() > 33_554_432
        {
            return Err(GovernanceError::Limit(
                "snapshot node/edge/source budget".into(),
            ));
        }
        if snapshot.diagnostics.len() > 256
            || snapshot.diagnostics.iter().any(|d| d.message.len() > 4096)
        {
            return Err(GovernanceError::Limit("analysis diagnostics budget".into()));
        }
        if snapshot.sources.keys().any(|p| {
            std::path::Path::new(p).is_absolute()
                || std::path::Path::new(p)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                || p.chars().any(char::is_control)
        }) {
            return Err(GovernanceError::Scope(
                "snapshot source paths must be relative and confined".into(),
            ));
        }
        if snapshot.source_coverage.len() > 20_000
            || snapshot
                .source_coverage
                .iter()
                .any(|c| c.path.len() > 4096 || c.domain.len() > 128 || c.relation.len() > 32)
        {
            return Err(GovernanceError::Limit(
                "source coverage metadata budget".into(),
            ));
        }
        nexora_graph::GraphIntegrityValidator::validate(&snapshot.graph)
            .map_err(|error| GovernanceError::Configuration(error.to_string()))?;
        Ok(Self {
            snapshot,
            language,
            organization,
            coverage: Default::default(),
            module_coverage: snapshot
                .source_coverage
                .iter()
                .filter(|c| c.complete && snapshot.sources.contains_key(&c.path))
                .map(|c| (c.path.clone(), c.relation.clone(), c.domain.clone()))
                .collect(),
            node_index: snapshot
                .graph
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.clone(), i))
                .collect(),
        })
    }
    pub fn node(&self, id: &nexora_core::NodeId) -> Option<&CodeNode> {
        self.node_index
            .get(id)
            .and_then(|i| self.snapshot.graph.nodes.get(*i))
    }
    pub fn validate_coverage(&self) -> Result<(), GovernanceError> {
        if self.coverage.relations.len() > 30_000 {
            return Err(GovernanceError::Limit("coverage records > 30000".into()));
        }
        for record in &self.coverage.relations {
            let node = self.node(&record.node_id).ok_or_else(|| {
                GovernanceError::Scope("coverage references a node outside this snapshot".into())
            })?;
            let supported = match record.relation {
                EdgeKind::Declares => record.domain == "recognized-top-level-declarations/v1",
                EdgeKind::Imports | EdgeKind::DependsOn => {
                    record.domain == "static-module-references/v1"
                        && self.module_coverage.contains(&(
                            node.name.clone(),
                            match record.relation {
                                EdgeKind::Imports => "IMPORTS",
                                _ => "DEPENDS_ON",
                            }
                            .into(),
                            record.domain.clone(),
                        ))
                }
                _ => false,
            };
            if record.domain.len() > 128
                || (record.exhaustive && (node.kind != NodeKind::File || !supported))
            {
                return Err(GovernanceError::Configuration(
                    "unsupported exhaustive coverage declaration".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn authorize(&self, scope: &PolicyScope) -> Result<(), GovernanceError> {
        if scope
            .organization
            .as_deref()
            .is_some_and(|o| Some(o) != self.organization)
            || (!scope.tenants.is_empty() && !scope.tenants.contains(&self.snapshot.tenant_id.0))
            || (!scope.repositories.is_empty()
                && !scope.repositories.contains(&self.snapshot.repository_id.0))
        {
            return Err(GovernanceError::Scope(
                "policy is outside the execution scope".into(),
            ));
        }
        Ok(())
    }
    pub fn anchor(&self, node: &CodeNode) -> NodeAnchor {
        if let Some(e) = &node.evidence {
            let verified = self
                .snapshot
                .sources
                .get(&e.path)
                .and_then(|s| s.get(e.span.start.offset..e.span.end.offset))
                .is_some();
            return NodeAnchor {
                path: Some(e.path.clone()),
                span: Some(e.span),
                verified,
            };
        }
        if node.kind == NodeKind::SyntaxUnit
            && let Some(origin) = self.snapshot.graph.syntax_units.get(&node.id)
        {
            return NodeAnchor {
                path: Some(origin.path.clone()),
                span: Some(origin.span),
                verified: self
                    .snapshot
                    .sources
                    .get(&origin.path)
                    .and_then(|source| source.get(origin.span.start.offset..origin.span.end.offset))
                    .is_some(),
            };
        }
        if node.kind == NodeKind::File
            && let Some(source) = self.snapshot.sources.get(&node.name)
        {
            let mut end = SourcePosition::default();
            end.advance(source);
            return NodeAnchor {
                path: Some(node.name.clone()),
                span: Some(SourceSpan {
                    start: SourcePosition::default(),
                    end,
                }),
                verified: true,
            };
        }
        NodeAnchor {
            path: None,
            span: None,
            verified: node.kind == NodeKind::Repository,
        }
    }
    pub fn exhaustive(&self, node: &CodeNode, kind: &EdgeKind) -> bool {
        self.coverage
            .relations
            .iter()
            .any(|c| c.node_id == node.id && c.relation == *kind && c.exhaustive)
            && node.kind == NodeKind::File
            && self.anchor(node).verified
            && !self
                .snapshot
                .diagnostics
                .iter()
                .any(|d| d.path.as_deref().is_none_or(|p| p == node.name))
    }
}
