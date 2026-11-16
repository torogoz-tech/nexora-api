use serde_json::Value;
pub(crate) struct TaskLifecycleMcpCatalog;
impl TaskLifecycleMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/task_lifecycle_tools.json"))
                .expect("bundled task lifecycle tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/task_lifecycle_inputs.json"))
                .expect("bundled task lifecycle inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_task_pool_list"
                | "nexora_task_pool_options"
                | "nexora_task_take"
                | "nexora_task_general_complete"
                | "nexora_task_pause"
                | "nexora_task_resume"
                | "nexora_task_move"
                | "nexora_task_workflow_migrate"
        )
    }
}
