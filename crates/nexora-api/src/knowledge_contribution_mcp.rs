use crate::{
    ApiState, knowledge_contribution_handlers::KnowledgeContributionHandlers,
    knowledge_contribution_mcp_input::KnowledgeContributionMcpInput,
    knowledge_rag_query::KnowledgeRagQuery, mcp_tool_result::McpToolResult,
};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct KnowledgeContributionMcp;
impl KnowledgeContributionMcp {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/knowledge_contribution_tools.json"))
            .expect("knowledge tools")
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let input: KnowledgeContributionMcpInput =
            serde_json::from_value(args).map_err(|_| (-32602, "Invalid contribution arguments"))?;
        let result = match name {
            "nexora_knowledge_context_search" => {
                let q = input.query;
                KnowledgeRagQuery::search(state, ctx, q.project_id, q.search.unwrap_or_default())
                    .await
            }
            "nexora_knowledge_contributions_list" => {
                let q = input.query;
                state
                    .run(move |store| {
                        store.knowledge_contributions(
                            &ctx,
                            q.project_id.as_deref(),
                            q.kind.as_deref().unwrap_or("document"),
                            q.search.as_deref().unwrap_or(""),
                            q.after.as_deref(),
                            q.descending.unwrap_or(false),
                        )
                    })
                    .await
            }
            "nexora_knowledge_contribution_get" => {
                let id = input
                    .contribution_id
                    .ok_or((-32602, "contribution_id required"))?;
                state
                    .run(move |store| store.knowledge_contribution(&ctx, &id, input.query.revision))
                    .await
            }
            "nexora_knowledge_contribute" => {
                KnowledgeContributionHandlers::write(
                    state,
                    ctx,
                    input.contribution_id,
                    input.request_id.ok_or((-32602, "request_id required"))?,
                    input
                        .contribution
                        .ok_or((-32602, "contribution required"))?,
                )
                .await
            }
            _ => return Err((-32601, "Unknown contribution tool")),
        };
        McpToolResult::output(result, structured)
    }
}
