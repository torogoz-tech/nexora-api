use serde_json::Value;
pub(crate) struct ApprovalRequestMcpCatalog;
impl ApprovalRequestMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/approval_request_tools.json"))
                .expect("bundled approval tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/approval_request_inputs.json"))
                .expect("bundled approval inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_approval_request_create"
                | "nexora_approval_request_cancel"
                | "nexora_approval_request_get"
                | "nexora_approval_requests_list"
        )
    }
}
