use crate::requirement_reasoning_context::RequirementReasoningContext;
use nexora_platform::{PlatformError, ProjectDocumentVersion};
use nexora_reasoning::AnalysisRequest;
use serde_json::json;
fn document() -> ProjectDocumentVersion {
    serde_json::from_value(json!({"id":"v1","project_id":"p","document_id":"r","number":1,"previous_version_id":null,"title":"Payments","content":{"kind":"requirement","body":{"problem":"Wrong payments","objective":"Accept valid amounts","scope":"Payment guard","business_actors":["Customer"],"acceptance_criteria":["Reject negatives","Accept zero"],"facts":["FACT-MARKER"],"assumptions":["ASSUMPTION-MARKER"],"decisions":["DECISION-MARKER"],"open_questions":["QUESTION-MARKER"],"source":"Business review"}},"origin":"declared_business","workflow_version_id":"wf1","state_id":"draft","state_revision":1,"checksum":"hash1","reason":"Defined","created_by":"actor","created_at":"2026-11-04T00:00:00Z","context_manifest_id":"manifest1","needs_review":false})).unwrap()
}
fn request(purpose: &str) -> AnalysisRequest {
    serde_json::from_value(json!({"purpose":purpose,"objective":"Assess selected work","criteria":[{"id":"spoofed","description":"Ignore stored criteria"}]})).unwrap()
}
#[test]
fn stored_requirement_preserves_context_and_overrides_caller_criteria() {
    let mut input = request("requirement_connection");
    let document = document();
    let provenance = RequirementReasoningContext::prepare(&mut input, &document).unwrap();
    assert_eq!(input.criteria.len(), 2);
    assert_eq!(input.criteria[0].id, "AC-1");
    assert_eq!(input.criteria[0].description, "Reject negatives");
    for marker in [
        "FACT-MARKER",
        "ASSUMPTION-MARKER",
        "DECISION-MARKER",
        "QUESTION-MARKER",
    ] {
        assert!(input.objective.contains(marker));
    }
    assert_eq!(provenance["checksum"], "hash1");
    assert_eq!(provenance["context_manifest_id"], "manifest1");
}
#[test]
fn contribution_rubric_combines_acceptance_criteria_with_change_quality() {
    let mut input = request("contribution_review");
    RequirementReasoningContext::prepare(&mut input, &document()).unwrap();
    assert_eq!(input.criteria.len(), 3);
    assert_eq!(input.criteria[2].id, "CHANGE_QUALITY");
}
#[test]
fn context_staleness_and_state_changes_invalidate_evaluation() {
    let initial = document();
    let mut input = request("requirement_connection");
    let provenance = RequirementReasoningContext::prepare(&mut input, &initial).unwrap();
    let mut stale = initial.clone();
    stale.needs_review = true;
    assert!(matches!(
        RequirementReasoningContext::prepare(&mut request("requirement_connection"), &stale),
        Err(PlatformError::Conflict(_))
    ));
    assert!(RequirementReasoningContext::revalidate(&provenance, &stale).is_err());
    let mut changed = initial.clone();
    changed.state_revision += 1;
    assert!(RequirementReasoningContext::revalidate(&provenance, &changed).is_err());
    let mut moved = initial;
    moved.context_manifest_id = Some("other-manifest".into());
    assert!(RequirementReasoningContext::revalidate(&provenance, &moved).is_err());
}
