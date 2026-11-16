mod support;
use nexora_core::TenantId;
use nexora_governance::*;
use nexora_graph::NodeKind;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
#[test]
fn resolved_exports_prove_existence_but_absence_has_no_exhaustive_coverage() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-export-policy-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let indexer = Indexer::new(language);
    for (source, status) in [
        (
            "function total() {}\nexport default total;\n",
            EvaluationStatus::Pass,
        ),
        ("function total() {}\n", EvaluationStatus::Unknown),
        ("export default total;\n", EvaluationStatus::Unknown),
    ] {
        fs::write(root.join("source.js"), source).unwrap();
        let snapshot = indexer
            .index(&root, TenantId("local".into()), &mut MemoryStore::default())
            .unwrap();
        let mut policy = support::policy();
        policy.spec.target.symbol = Some(NodeKind::File);
        policy.spec.target.filter = None;
        policy.spec.assert = support::assertion(
            "relation: EXPORTS\ntarget:\n  symbol: function\n  name: total\ntraversal:\n  maxDepth: 1\n",
        );
        assert_eq!(support::evaluate(&snapshot, policy).status, status);
    }
    fs::remove_dir_all(root).unwrap();
}
