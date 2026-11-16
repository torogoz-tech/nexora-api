use crate::{
    ApiState, code_mcp_query::CodeMcpQuery, knowledge_rag_query::KnowledgeRagQuery,
    mcp_tool_result::McpToolResult,
};
use nexora_platform::AuthContext;
use serde_json::Value;

pub(crate) struct CodeMcpTools;
impl CodeMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let input = CodeMcpQuery::parse(name, args)?;
        let query = input.query.clone();
        let project_id = input.project_id.clone();
        let code_ctx = ctx.clone();
        let result = state
            .run(move |store| {
                store.query_repository_code(
                    &code_ctx,
                    &input.project_id,
                    &input.repository_id,
                    input.snapshot.as_deref(),
                    &input.query,
                )
            })
            .await;
        let result = match (result, query) {
            (Ok(mut result), nexora_runtime::KnowledgeQuery::Search { query, .. }) => {
                let search = query[..query.floor_char_boundary(query.len().min(256))].to_owned();
                match KnowledgeRagQuery::search(state, ctx, Some(project_id), search).await {
                    Ok(context) => {
                        result["supporting_knowledge"] = context;
                        Ok(result)
                    }
                    Err(crate::api_error::ApiError(
                        nexora_platform::PlatformError::Forbidden
                        | nexora_platform::PlatformError::Validation(_),
                    )) => {
                        result["supporting_knowledge"] = serde_json::json!({"available":false});
                        Ok(result)
                    }
                    Err(error) => Err(error),
                }
            }
            (result, _) => result,
        };
        McpToolResult::output(result, structured)
    }
}
