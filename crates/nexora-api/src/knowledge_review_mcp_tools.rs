use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct KnowledgeReviewMcpTools;
impl KnowledgeReviewMcpTools {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/knowledge_review_tools.json"))
            .expect("knowledge review schemas")
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let schema = Self::list()
            .into_iter()
            .find(|t| t["name"] == name)
            .ok_or((-32601, "Unknown knowledge review tool"))?;
        let obj = args
            .as_object()
            .ok_or((-32602, "Arguments must be an object"))?;
        if obj
            .keys()
            .any(|k| schema["inputSchema"]["properties"].get(k).is_none())
        {
            return Err((-32602, "Unknown argument"));
        }
        let text = |k: &str| {
            args[k]
                .as_str()
                .map(str::to_owned)
                .ok_or((-32602, "Required string missing"))
        };
        let result = match name {
            "nexora_knowledge_review_request" => {
                let key = text("request_id")?;
                let mut input = args.clone();
                input.as_object_mut().unwrap().remove("request_id");
                state
                    .mutate(
                        ctx,
                        "knowledge.review.request",
                        key,
                        input.clone(),
                        move |s, c| s.request_knowledge_review(c, &input),
                    )
                    .await
            }
            "nexora_products_list" => state.run(move |s| s.products(&ctx, true)).await,
            "nexora_knowledge_review_get" => {
                let id = text("review_id")?;
                state.run(move |s| s.knowledge_review(&ctx, &id)).await
            }
            "nexora_knowledge_review_list" => {
                let resource = args["resource_id"].as_str().map(str::to_owned);
                let status = args["status"].as_str().unwrap_or("pending").to_owned();
                let page = args["page"].as_i64().unwrap_or(1);
                state
                    .run(move |s| s.knowledge_reviews(&ctx, resource.as_deref(), &status, page))
                    .await
            }
            _ => return Err((-32601, "Unknown tool")),
        };
        McpToolResult::output(result, structured)
    }
}
