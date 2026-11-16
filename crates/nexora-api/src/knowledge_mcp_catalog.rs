use serde_json::{Value, json};

pub(crate) struct KnowledgeMcpCatalog;
impl KnowledgeMcpCatalog {
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_list_pending"
                | "nexora_list_knowledge"
                | "nexora_get_knowledge"
                | "nexora_contribute_knowledge"
                | "nexora_revise_knowledge"
                | "nexora_review_knowledge"
        )
    }
    pub fn list() -> Vec<Value> {
        nexora_mcp::ToolCatalog::list_with_knowledge(false,true).into_iter().filter(|tool|tool["name"].as_str().is_some_and(Self::supports)).map(|mut tool| {
            let id=json!({"type":"string","minLength":1,"maxLength":128});
            tool["inputSchema"]["properties"]["project_id"]=id.clone();
            tool["inputSchema"]["properties"]["repository_id"]=id;
            tool["inputSchema"]["required"].as_array_mut().unwrap().extend([json!("project_id"),json!("repository_id")]);
            if let Some(request) = tool["inputSchema"]["properties"].get_mut("request_id") {
                request["pattern"]=json!("^[A-Za-z0-9._:-]+$");
                request["description"]=json!("Actor-scoped idempotency key; preserve on exact retry. Credential prefixes nxk_ and nxs_ are forbidden.");
            }
            tool["inputSchema"]["properties"]["snapshot"]["description"]=json!("Optional published snapshot in this registered repository and current source authority; omit for current. A confirmed mutation retry retains its original snapshot and response.");
            tool
        }).collect()
    }
}
