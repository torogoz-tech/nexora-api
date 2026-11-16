use crate::{
    ApiState, mcp_tool_result::McpToolResult, reasoning_handlers::ReasoningHandlers,
    reasoning_request::ReasoningRequest,
};
use nexora_platform::AuthContext;
use serde_json::{Value, json};
pub(crate) struct ReasoningMcpTools;
impl ReasoningMcpTools {
    pub fn catalog() -> Value {
        json!({"name":"nexora_evaluate_repository","description":"Use installation-owned OpenAI reasoning to understand code, assess acceptance criteria, review code or documented contributions. Sends authorized source evidence to OpenAI; returns inferred findings, never human approval. Requires operator reasoning settings; bounded and may take longer than code queries.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","repository_id","analysis"],"properties":{"project_id":{"type":"string","minLength":1,"maxLength":128},"repository_id":{"type":"string","minLength":1,"maxLength":128},"requirement":{"type":"object","additionalProperties":false,"required":["document_id","version_id"],"properties":{"document_id":{"type":"string"},"version_id":{"type":"string"}}},"evidence_ids":{"type":"array","maxItems":6,"items":{"type":"string","minLength":1,"maxLength":128}},"snapshot":{"type":"string"},"baseline_snapshot":{"type":"string","description":"Optional independently authorized baseline in the same registration; enables bounded comparison. Does not prove author identity or chronological ordering."},"analysis":{"type":"object","additionalProperties":false,"required":["purpose","objective","criteria"],"properties":{"purpose":{"type":"string","enum":["understanding","requirement_connection","code_review","contribution_review"]},"objective":{"type":"string","minLength":1,"maxLength":16000},"criteria":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","additionalProperties":false,"required":["id","description"],"properties":{"id":{"type":"string","minLength":1,"maxLength":128},"description":{"type":"string","minLength":1,"maxLength":8000}}}}}}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":false,"openWorldHint":true}})
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = || (-32602, "Invalid repository evaluation arguments");
        let mut args = args.as_object().cloned().ok_or_else(invalid)?;
        let project = args
            .remove("project_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid)?;
        let repository = args
            .remove("repository_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid)?;
        if project.is_empty()
            || project.len() > 128
            || repository.is_empty()
            || repository.len() > 128
        {
            return Err(invalid());
        }
        let input: ReasoningRequest =
            serde_json::from_value(Value::Object(args)).map_err(|_| invalid())?;
        input.analysis.validate().map_err(|_| invalid())?;
        McpToolResult::output(
            ReasoningHandlers::run(state, ctx, project, repository, input).await,
            structured,
        )
    }
}
