use crate::code_mcp_query::CodeMcpQuery;
use serde_json::{Value, json};

pub(crate) struct CodeMcpCatalog;
impl CodeMcpCatalog {
    pub fn list() -> Vec<Value> {
        nexora_mcp::ToolCatalog::list(false).into_iter()
            .filter(|tool| tool["name"].as_str().is_some_and(CodeMcpQuery::supports))
            .map(|mut tool| {
                let id = json!({"type":"string","minLength":1,"maxLength":128});
                tool["inputSchema"]["properties"]["project_id"] = id.clone();
                tool["inputSchema"]["properties"]["repository_id"] = id;
                if let Some(required) = tool["inputSchema"]["required"].as_array_mut() {
                    required.extend([json!("project_id"), json!("repository_id")]);
                }
                tool["inputSchema"]["properties"]["snapshot"]["description"] = json!("Optional published snapshot in this registration and current source authority. Runtime-ready unpublished results are inaccessible. Local snapshots require explicit current-epoch operator attestation.");
                tool
            }).collect()
    }
}
