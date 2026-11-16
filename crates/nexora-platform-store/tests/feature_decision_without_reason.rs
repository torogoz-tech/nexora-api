#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::FeatureProposalDecision;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL"]
fn feature_decisions_do_not_require_a_reason_and_keep_the_decision_audit() {
    let mut fixture = FeatureMapFixture::new();
    for decision in ["rejected", "approved"] {
        let input = fixture.proposal("Reason-free decision", false);
        let proposal = fixture
            .work
            .store
            .propose_feature_map(&fixture.pmo, &fixture.work.project.id, &input)
            .unwrap();
        let decision_input: FeatureProposalDecision = serde_json::from_value(json!({
            "expected_revision": proposal["revision"], "decision": decision,
        }))
        .unwrap();
        let result = fixture
            .work
            .store
            .decide_feature_map(
                &fixture.pmo,
                &fixture.work.project.id,
                proposal["id"].as_str().unwrap(),
                &decision_input,
            )
            .unwrap();
        assert_eq!(result["status"], decision);
        assert_eq!(result["decided_by"], fixture.pmo.actor_id());
        assert!(result["decided_at"].is_string());
        assert_eq!(result["decision_reason"], "");
    }
}
