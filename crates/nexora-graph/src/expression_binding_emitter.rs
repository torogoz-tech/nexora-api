use crate::{
    CodeEdge, CodeGraph, CodeNode, EdgeKind, Evidence, NodeKind, ResolutionState,
    expression_binding_result::ExpressionBindingResult,
};
use nexora_ast::AbstractSyntaxNode;
use nexora_core::{AnalysisDiagnostic, EdgeId, NodeId};
pub(crate) struct ExpressionBindingEmitter<'a> {
    pub graph: &'a mut CodeGraph,
    pub path: &'a str,
    pub version: &'a str,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}
impl ExpressionBindingEmitter<'_> {
    pub fn emit(
        &mut self,
        node: &AbstractSyntaxNode,
        kind: EdgeKind,
        label: &str,
        result: &ExpressionBindingResult,
    ) {
        let path = self.path;
        let version = self.version;
        let graph = &mut self.graph;
        let diagnostics = &mut self.diagnostics;
        let site = NodeId(format!("{path}:{}", node.id.0));
        let evidence = Evidence {
            syntax_unit: None,
            path: path.into(),
            span: node.span,
            ast_id: node.id.clone(),
            rule_version: version.into(),
        };
        let relation = if kind == EdgeKind::Calls {
            "CALLS"
        } else {
            "REFERENCES"
        };
        let target = result
            .target
            .clone()
            .unwrap_or_else(|| NodeId(format!("binding:{relation}:{path}:{}", node.id.0)));
        if result.target.is_none() {
            graph.nodes.push(CodeNode {
                expression_policy: None,
                mutation_operand: None,
                syntax_unit: None,
                id: target.clone(),
                kind: NodeKind::Symbol,
                name: node.name.clone(),
                expression: None,
                lexical_region: None,
                evidence: Some(evidence.clone()),
                scope_id: None,
                owned_scope_id: None,
                scope_policy: None,
                regions: Vec::new(),
                captures: Default::default(),
            });
        }
        graph.edges.push(CodeEdge {
            call_binding: None,
            scope_binding: None,
            id: EdgeId(format!("{path}:expression:{relation}:{}", node.id.0)),
            source: site,
            target,
            kind,
            evidence: evidence.clone(),
            resolution: result.state.clone(),
            resolution_reason: Some(result.reason.into()),
            label: Some(label.into()),
        });
        if result.state != ResolutionState::Resolved && diagnostics.len() < 1024 {
            diagnostics.push(AnalysisDiagnostic {
                stage: "expression-resolution".into(),
                path: Some(path.into()),
                span: Some(evidence.span),
                message: result.reason.into(),
            });
        }
    }
}
