use nexora_runtime::KnowledgeCommand;
use serde_json::{Value, json};

pub(crate) struct KnowledgeMcpInput {
    pub project_id: String,
    pub repository_id: String,
    pub snapshot: Option<String>,
    pub command: Option<KnowledgeCommand>,
    pub offset: usize,
    pub limit: usize,
}
impl KnowledgeMcpInput {
    pub fn parse(name: &str, args: Value) -> Result<Self, (i64, &'static str)> {
        let invalid = || (-32602, "Invalid registered knowledge arguments");
        let mut args = args.as_object().cloned().ok_or_else(invalid)?;
        let project_id = args
            .remove("project_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid)?;
        let repository_id = args
            .remove("repository_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid)?;
        if project_id.is_empty()
            || project_id.len() > 128
            || repository_id.is_empty()
            || repository_id.len() > 128
        {
            return Err(invalid());
        }
        nexora_mcp::ToolCatalog::validate_with_knowledge(
            name,
            &Value::Object(args.clone()),
            false,
            true,
        )
        .map_err(|_| invalid())?;
        let snapshot = args
            .remove("snapshot")
            .and_then(|v| v.as_str().map(str::to_owned));
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
        let command = if name == "nexora_list_pending" {
            None
        } else {
            let operation = match name {
                "nexora_list_knowledge" => "list",
                "nexora_get_knowledge" => "get",
                "nexora_contribute_knowledge" => "contribute",
                "nexora_revise_knowledge" => "revise",
                "nexora_review_knowledge" => "review",
                _ => return Err(invalid()),
            };
            args.insert("operation".into(), json!(operation));
            Some(serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?)
        };
        Ok(Self {
            project_id,
            repository_id,
            snapshot,
            command,
            offset,
            limit,
        })
    }
}
