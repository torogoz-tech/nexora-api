use serde_json::Value;
pub(crate) struct WorkCatalogMcpCatalog;
impl WorkCatalogMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/work_catalog_tools.json"))
                .expect("bundled work catalog tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/work_catalog_inputs.json"))
                .expect("bundled work catalog inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_task_types_list" | "nexora_backlogs_list" | "nexora_task_type_change"
        )
    }
}
