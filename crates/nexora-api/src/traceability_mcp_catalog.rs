use serde_json::Value;
pub(crate) struct TraceabilityMcpCatalog;
impl TraceabilityMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/traceability_tools.json"))
                .expect("bundled traceability tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/document_inputs.json"))
                .expect("bundled traceability inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_document_link_create"
                | "nexora_document_link_remove"
                | "nexora_context_package_create"
                | "nexora_document_links_list"
                | "nexora_task_documents_list"
                | "nexora_context_package_get"
                | "nexora_context_package_export"
        )
    }
}
