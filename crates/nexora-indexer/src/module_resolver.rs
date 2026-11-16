use crate::{ModuleResolution, ModuleSpecifier};
use nexora_core::{AnalysisDiagnostic, NodeId};
use nexora_graph::{CodeGraph, EdgeKind, NodeKind, ResolutionState};
use std::collections::{BTreeMap, BTreeSet};
pub struct ModuleResolver;
impl ModuleResolver {
    pub fn resolve(
        graph: &mut CodeGraph,
        manifest: &nexora_language::LanguageManifest,
        diagnostics: &mut Vec<AnalysisDiagnostic>,
    ) -> Result<(), nexora_core::NexoraError> {
        let plan = manifest
            .resolution
            .as_ref()
            .map(|d| crate::ResolutionPlan::compile(d, &manifest.extensions))
            .transpose()?;
        Self::resolve_prepared(graph, manifest, diagnostics, plan)
    }
    pub(crate) fn resolve_prepared(
        graph: &mut CodeGraph,
        manifest: &nexora_language::LanguageManifest,
        diagnostics: &mut Vec<AnalysisDiagnostic>,
        mut plan: Option<crate::ResolutionPlan>,
    ) -> Result<(), nexora_core::NexoraError> {
        let nodes: BTreeMap<_, _> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let files: BTreeMap<_, _> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::File)
            .map(|n| (n.name.clone(), n.id.clone()))
            .collect();
        let mut references: BTreeMap<NodeId, (String, Vec<usize>)> = BTreeMap::new();
        for (i, edge) in graph.edges.iter().enumerate() {
            if !matches!(edge.kind, EdgeKind::Imports | EdgeKind::DependsOn)
                || edge.resolution != ResolutionState::Unresolved
            {
                continue;
            }
            let Some(target) = nodes.get(&edge.target).and_then(|i| graph.nodes.get(*i)) else {
                continue;
            };
            let Some(source) = nodes.get(&edge.source).and_then(|i| graph.nodes.get(*i)) else {
                continue;
            };
            let origin = source
                .syntax_unit
                .as_ref()
                .and_then(|id| graph.syntax_units.get(id));
            if origin.is_some_and(|unit| unit.language != manifest.id) {
                continue;
            }
            if target.kind == NodeKind::Module
                && matches!(source.kind, NodeKind::File | NodeKind::SyntaxUnit)
            {
                references
                    .entry(target.id.clone())
                    .or_insert_with(|| {
                        (
                            origin.map_or_else(|| source.name.clone(), |unit| unit.path.clone()),
                            vec![],
                        )
                    })
                    .1
                    .push(i);
            }
        }
        let mut removed = BTreeSet::new();
        for (id, (source, edges)) in references {
            let Some(index) = nodes.get(&id).copied() else {
                continue;
            };
            let raw = graph.nodes[index].name.clone();
            let result = match manifest
                .resolution
                .as_ref()
                .ok_or_else(|| "no literal strategy configured".to_owned())
                .and_then(|d| ModuleSpecifier::decode(&raw, &d.literal))
            {
                Ok(specifier) => {
                    graph.nodes[index].name = specifier.clone();
                    plan.as_mut().map_or_else(
                        || ModuleResolution::Unresolved("no resolution strategy configured".into()),
                        |plan| plan.find(&source, &specifier, &files),
                    )
                }
                Err(reason) => ModuleResolution::Unresolved(reason),
            };
            match result {
                ModuleResolution::Resolved(target) => {
                    for i in edges {
                        graph.edges[i].target = target.clone();
                        graph.edges[i].resolution = ResolutionState::Resolved;
                        if graph.edges[i].evidence.syntax_unit.is_some() {
                            graph.edges[i].resolution_reason =
                                Some("declared_module_resolution/v1".into());
                        }
                    }
                    removed.insert(id);
                }
                unresolved => {
                    let (state, message) = match unresolved {
                        ModuleResolution::Ambiguous(paths) => (
                            ResolutionState::Ambiguous,
                            format!("ambiguous local module ({} candidates)", paths.len()),
                        ),
                        ModuleResolution::Unresolved(reason) => {
                            (ResolutionState::Unresolved, reason)
                        }
                        _ => unreachable!(),
                    };
                    for i in edges {
                        graph.edges[i].resolution = state.clone();
                    }
                    diagnostics.push(AnalysisDiagnostic {
                        stage: "module-resolution".into(),
                        path: Some(source),
                        message,
                        span: graph.nodes[index].evidence.as_ref().map(|e| e.span),
                    });
                }
            }
        }
        graph.nodes.retain(|n| !removed.contains(&n.id));
        Ok(())
    }
}
