use crate::knowledge_contribution_mcp_input::KnowledgeContributionMcpInput;
use serde_json::json;
#[test]
fn mcp_accepts_versioned_uploads_and_rejects_unknown_fields() {
    let input:KnowledgeContributionMcpInput=serde_json::from_value(json!({"request_id":"source-update","contribution_id":"knc_example","contribution":{"kind":"document","title":"Manual","filename":"manual.pdf","data_base64":"JVBERi0=","expected_revision":2}})).unwrap();
    assert_eq!(input.contribution.unwrap().expected_revision, Some(2));
    let input: KnowledgeContributionMcpInput = serde_json::from_value(
        json!({"project_id":"prj_example","kind":"document","after":"knc_example"}),
    )
    .unwrap();
    assert_eq!(input.query.kind.as_deref(), Some("document"));
    assert!(
        serde_json::from_value::<KnowledgeContributionMcpInput>(json!({"unknown":true})).is_err()
    );
}
