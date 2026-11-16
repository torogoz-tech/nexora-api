use crate::reasoning_request::ReasoningRequest;
use serde_json::json;
#[test]
fn reasoning_contract_rejects_scope_and_provider_injection() {
    let base = json!({"analysis":{"purpose":"code_review","objective":"Review","criteria":[{"id":"C","description":"Guard"}]}});
    assert!(serde_json::from_value::<ReasoningRequest>(base.clone()).is_ok());
    for field in [
        "actor_id",
        "project_id",
        "repository_id",
        "endpoint",
        "api_key",
        "model",
    ] {
        let mut value = base.clone();
        value[field] = json!("injected");
        assert!(serde_json::from_value::<ReasoningRequest>(value).is_err());
    }
    let mut request: ReasoningRequest = serde_json::from_value(base).unwrap();
    request
        .analysis
        .criteria
        .push(request.analysis.criteria[0].clone());
    assert!(request.analysis.validate().is_err());
}
#[test]
fn reasoning_catalog_has_explicit_external_effect_and_four_purposes() {
    let tool = crate::reasoning_mcp_tools::ReasoningMcpTools::catalog();
    assert_eq!(tool["annotations"]["openWorldHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(
        tool["inputSchema"]["properties"]["analysis"]["properties"]["purpose"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
