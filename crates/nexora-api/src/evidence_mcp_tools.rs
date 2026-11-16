use crate::{ApiState, evidence_handlers::EvidenceHandlers, mcp_tool_result::McpToolResult};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct EvidenceMcpTools;
impl EvidenceMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = || (-32602, "Invalid evidence arguments");
        match name {
            "nexora_evidence_upload" => {
                let input: crate::mcp_evidence_upload::McpEvidenceUpload =
                    serde_json::from_value(args).map_err(|_| invalid())?;
                let result = EvidenceHandlers::save(
                    state,
                    ctx,
                    input.project_id,
                    input.request_id,
                    input.evidence,
                )
                .await
                .and_then(|record| {
                    serde_json::to_value(record).map_err(|_| {
                        crate::api_error::ApiError(nexora_platform::PlatformError::Storage)
                    })
                });
                McpToolResult::output(result, structured)
            }
            "nexora_evidence_get" => {
                let input: crate::mcp_evidence_get::McpEvidenceGet =
                    serde_json::from_value(args).map_err(|_| invalid())?;
                match state
                    .run(move |store| {
                        store.evidence_content(&ctx, &input.project_id, &input.evidence_id)
                    })
                    .await
                {
                    Ok(content) => EvidenceHandlers::mcp_content(
                        content,
                        input.include_content.unwrap_or(false),
                        structured,
                    ),
                    Err(error) => McpToolResult::output(Err(error), structured),
                }
            }
            "nexora_evidence_list" => {
                let input: crate::mcp_evidence_list::McpEvidenceList =
                    serde_json::from_value(args).map_err(|_| invalid())?;
                McpToolResult::output(
                    state
                        .run(move |store| {
                            store.evidence_page(
                                &ctx,
                                &input.project_id,
                                &input.repository_id,
                                input.after.as_deref(),
                                input.limit.unwrap_or(20),
                            )
                        })
                        .await,
                    structured,
                )
            }
            _ => Err((-32601, "Unknown evidence tool")),
        }
    }
}
