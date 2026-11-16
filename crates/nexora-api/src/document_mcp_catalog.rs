use serde_json::Value;
pub(crate) struct DocumentMcpCatalog;
impl DocumentMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/document_tools.json"))
                .expect("bundled document tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/document_inputs.json"))
                .expect("bundled document inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools.retain(|tool| tool["name"] != "nexora_document_publish");
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_document_create"
                | "nexora_document_change"
                | "nexora_document_revise"
                | "nexora_document_publish"
                | "nexora_document_get"
                | "nexora_document_version_get"
                | "nexora_document_versions_list"
                | "nexora_document_export"
                | "nexora_documents_list"
                | "nexora_document_history"
                | "nexora_document_publications_list"
        )
    }
}
