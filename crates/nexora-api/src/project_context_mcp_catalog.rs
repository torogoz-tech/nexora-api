use serde_json::Value;
pub(crate) struct ProjectContextMcpCatalog;
impl ProjectContextMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/project_context_tools.json"))
                .expect("bundled context tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/document_inputs.json"))
                .expect("bundled context inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_context_manifest_create"
                | "nexora_context_manifest_get"
                | "nexora_context_manifest_status"
        )
    }
}
