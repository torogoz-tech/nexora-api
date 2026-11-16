use serde_json::Value;
pub(crate) struct TaskTemplateMcpCatalog;
impl TaskTemplateMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/task_template_tools.json"))
                .expect("bundled template tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/work_item_inputs.json"))
                .expect("bundled template inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_task_template_create"
                | "nexora_task_template_change"
                | "nexora_task_template_publish"
                | "nexora_task_template_apply"
                | "nexora_task_template_get"
                | "nexora_task_template_version_get"
                | "nexora_task_templates_list"
                | "nexora_task_template_versions_list"
        )
    }
}
