use serde_json::Value;
pub(crate) struct WorkItemMcpCatalog;
impl WorkItemMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/work_item_tools.json"))
                .expect("bundled work-item tool schemas");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/work_item_inputs.json"))
                .expect("bundled work-item input schemas");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_task_approval_get"
                | "nexora_task_type_create"
                | "nexora_backlog_create"
                | "nexora_backlog_change"
                | "nexora_backlog_reorder"
                | "nexora_task_create"
                | "nexora_task_change"
                | "nexora_task_transition"
                | "nexora_task_transition_runs"
                | "nexora_task_transition_retry"
                | "nexora_task_get"
                | "nexora_backlog_get"
                | "nexora_tasks_list"
                | "nexora_task_history"
        )
    }
}
