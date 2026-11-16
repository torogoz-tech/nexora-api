#![allow(dead_code)]
use nexora_core::TenantId;
use nexora_governance::*;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use nexora_storage::Snapshot;
use std::path::Path;
pub fn policy() -> Policy {
    GovernanceIo::yaml(
        &Path::new("../../policies/examples/naming.policy.yaml")
            .canonicalize()
            .unwrap(),
    )
    .unwrap()
}
pub fn snapshot(case: &str) -> Snapshot {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    Indexer::new(language)
        .index(
            &Path::new("../../policies/examples/fixtures/naming").join(case),
            TenantId("local".into()),
            &mut MemoryStore::default(),
        )
        .unwrap()
}
pub fn evaluate(snapshot: &Snapshot, policy: Policy) -> EvaluationResult {
    let mut context = EvaluationContext::new(
        snapshot,
        &snapshot.tenant_id,
        &snapshot.repository_id,
        "javascript",
        None,
    )
    .unwrap();
    context.coverage = AnalysisCoverage::indexed(snapshot);
    PolicyEvaluator::evaluate(&context, &[PolicyCompiler::compile(policy).unwrap()]).unwrap()
}
pub fn assertion(yaml: &str) -> PolicyAssertion {
    serde_yaml_ng::from_str(yaml).unwrap()
}
