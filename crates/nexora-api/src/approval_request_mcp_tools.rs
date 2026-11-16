use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct ApprovalRequestMcpTools;
impl ApprovalRequestMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid approval request arguments");
        let result = match name {
            "nexora_approval_request_create" => {
                let input: crate::mcp_approval_request_create::McpApprovalRequestCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "approval_request.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_approval_request(
                                ctx,
                                &input.project_id,
                                &input.definition,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_approval_request_cancel" => {
                let input: crate::mcp_approval_request_cancel::McpApprovalRequestCancel =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"approval_request_id":input.approval_request_id,"cancellation":input.cancellation});
                state
                    .mutate(
                        ctx,
                        "approval_request.cancel",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.cancel_approval_request(
                                ctx,
                                &input.project_id,
                                &input.approval_request_id,
                                &input.cancellation,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_approval_request_get" => {
                let input: crate::mcp_approval_request_get::McpApprovalRequestGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_approval_request(
                            &ctx,
                            &input.project_id,
                            &input.approval_request_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_approval_requests_list" => {
                let input: crate::mcp_approval_requests_list::McpApprovalRequestsList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.approval_request_page(
                            &ctx,
                            &input.project_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                            input.status,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown approval request tool")),
        };
        McpToolResult::output(result, structured)
    }
}
