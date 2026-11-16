use nexora_core::TenantId;
use nexora_governance::*;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::Path;
#[test]
fn same_policy_has_the_same_meaning_on_javascript_and_lua() {
    let root = Path::new("../../policies/architecture/language-neutral-boundary")
        .canonicalize()
        .unwrap();
    let registry = PolicyRegistry::load(&root).unwrap();
    let policies: Vec<_> = registry.policies().cloned().collect();
    let mut fingerprint = None;
    for language_id in ["javascript", "lua"] {
        let language =
            LanguageDefinition::load(&Path::new("../../languages").join(language_id)).unwrap();
        for (case, status) in [
            ("pass", EvaluationStatus::Pass),
            ("fail", EvaluationStatus::Fail),
            ("unknown", EvaluationStatus::Unknown),
        ] {
            let snapshot = Indexer::new(language.clone())
                .index(
                    &root.join("fixtures").join(language_id).join(case),
                    TenantId("local".into()),
                    &mut MemoryStore::default(),
                )
                .unwrap();
            let mut context = EvaluationContext::new(
                &snapshot,
                &snapshot.tenant_id,
                &snapshot.repository_id,
                language_id,
                None,
            )
            .unwrap();
            context.coverage = AnalysisCoverage::indexed(&snapshot);
            let result = PolicyEvaluator::evaluate(&context, &policies).unwrap();
            assert_eq!(result.status, status, "{language_id}/{case}");
            if let Some(hash) = &fingerprint {
                assert_eq!(hash, &result.policy_set_hash);
            } else {
                fingerprint = Some(result.policy_set_hash);
            }
            assert!(result.findings.iter().all(|f| !f.evidence.is_empty()));
        }
    }
}
