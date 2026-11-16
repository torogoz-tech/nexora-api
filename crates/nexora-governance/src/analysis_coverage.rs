use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisCoverage {
    pub relations: Vec<crate::RelationCoverage>,
}
impl AnalysisCoverage {
    pub fn indexed(snapshot: &nexora_storage::Snapshot) -> Self {
        let mut by_file: std::collections::BTreeMap<&str, Vec<&nexora_core::SourceCoverage>> =
            std::collections::BTreeMap::new();
        for record in &snapshot.source_coverage {
            by_file.entry(&record.path).or_default().push(record);
        }
        let mut relations = Vec::new();
        for node in snapshot
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == nexora_graph::NodeKind::File)
            .filter(|n| snapshot.sources.contains_key(&n.name))
        {
            let clean = !snapshot
                .diagnostics
                .iter()
                .any(|d| d.path.as_deref().is_none_or(|p| p == node.name))
                && snapshot
                    .graph
                    .syntax_units
                    .get(&node.id)
                    .is_none_or(|unit| unit.status == nexora_core::RegionStatus::Parsed);
            if clean {
                relations.push(crate::RelationCoverage {
                    node_id: node.id.clone(),
                    relation: nexora_graph::EdgeKind::Declares,
                    exhaustive: true,
                    domain: "recognized-top-level-declarations/v1".into(),
                });
            }
            for record in by_file
                .get(node.name.as_str())
                .into_iter()
                .flatten()
                .filter(|c| c.domain == "static-module-references/v1")
            {
                let relation = match record.relation.as_str() {
                    "IMPORTS" => nexora_graph::EdgeKind::Imports,
                    "DEPENDS_ON" => nexora_graph::EdgeKind::DependsOn,
                    _ => continue,
                };
                relations.push(crate::RelationCoverage {
                    node_id: node.id.clone(),
                    relation,
                    exhaustive: record.complete && clean,
                    domain: record.domain.clone(),
                });
            }
        }
        Self { relations }
    }
}
