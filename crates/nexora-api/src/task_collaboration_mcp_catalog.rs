use serde_json::Value;
pub(crate) struct TaskCollaborationMcpCatalog;
impl TaskCollaborationMcpCatalog {
    pub fn list() -> Vec<Value> {
        let mut tools: Vec<Value> =
            serde_json::from_str(include_str!("../schemas/task_collaboration_tools.json"))
                .expect("bundled collaboration tools");
        let definitions: Value =
            serde_json::from_str(include_str!("../schemas/work_item_inputs.json"))
                .expect("bundled work-item inputs");
        crate::mcp_schema_definitions::McpSchemaDefinitions::attach(&mut tools, &definitions);
        tools
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_task_commit_remove"
                | "nexora_task_commits_list"
                | "nexora_task_commit_bind"
                | "nexora_release_compose"
                | "nexora_release_composition_get"
                | "nexora_releases_list"
                | "nexora_release_get"
                | "nexora_release_create"
                | "nexora_release_tasks_add"
                | "nexora_task_work_logs_list"
                | "nexora_task_work_log_add"
                | "nexora_task_work_log_change"
                | "nexora_task_work_log_remove"
                | "nexora_task_project_change"
                | "nexora_task_knowledge_link"
                | "nexora_task_relations_get"
                | "nexora_task_stages_list"
                | "nexora_task_subtask_create"
                | "nexora_task_subtask_link"
                | "nexora_task_subtask_remove"
                | "nexora_task_dependency_create"
                | "nexora_task_dependency_resolve"
                | "nexora_task_dependency_remove"
                | "nexora_task_dependencies_list"
                | "nexora_task_comment_add"
                | "nexora_task_comments_list"
        )
    }
}
