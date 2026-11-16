use crate::{
    ApiState, mcp_tool_result::McpToolResult,
    organization_knowledge_mcp_input::OrganizationKnowledgeMcpInput,
};
use nexora_platform::AuthContext;
use serde_json::{Value, json};
pub(crate) struct OrganizationKnowledgeMcp;
impl OrganizationKnowledgeMcp {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/organization_knowledge_tools.json"))
            .expect("organization knowledge tools")
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let input: OrganizationKnowledgeMcpInput = serde_json::from_value(args)
            .map_err(|_| (-32602, "Invalid organization knowledge input"))?;
        let result = match name {
            "nexora_organization_documents_list" => {
                state
                    .run(move |store| {
                        store.organization_knowledge_page(
                            &ctx,
                            input.kind.as_deref().unwrap_or("document"),
                            input.search.as_deref().unwrap_or(""),
                            input.after.as_deref(),
                        )
                    })
                    .await
            }
            "nexora_organization_document_get" => {
                let id = input.document_id.ok_or((-32602, "document_id required"))?;
                state
                    .run(move |store| store.organization_knowledge_get(&ctx, &id, input.revision))
                    .await
            }
            "nexora_organization_document_write" => {
                let request = input.request_id.ok_or((-32602, "request_id required"))?;
                let document = input.document.ok_or((-32602, "document required"))?;
                let canonical = json!({"id":input.document_id,"document":document});
                state
                    .mutate(
                        ctx,
                        "organization.document.write",
                        request,
                        canonical,
                        move |store, ctx| {
                            store.organization_knowledge_write(
                                ctx,
                                input.document_id.as_deref(),
                                &document,
                            )
                        },
                    )
                    .await
            }
            _ => return Err((-32601, "Unknown organization knowledge tool")),
        };
        McpToolResult::output(result, structured)
    }
}
