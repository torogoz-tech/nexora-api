use crate::{
    ApiState, mcp_context_manifest_create::McpContextManifestCreate,
    mcp_context_manifest_get::McpContextManifestGet,
    mcp_context_manifest_status::McpContextManifestStatus, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct ProjectContextMcpTools;
impl ProjectContextMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid context tool arguments");
        let result = match name {
            "nexora_context_manifest_create" => {
                let input: McpContextManifestCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"selection":input.selection});
                state
                    .mutate(
                        ctx,
                        "context_manifest.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_project_context_manifest(
                                ctx,
                                &input.project_id,
                                &input.selection,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_context_manifest_get" => {
                let input: McpContextManifestGet = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_project_context_manifest(
                            &ctx,
                            &input.project_id,
                            &input.manifest_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_context_manifest_status" => {
                let input: McpContextManifestStatus =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.project_context_manifest_status(
                            &ctx,
                            &input.project_id,
                            &input.manifest_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown context tool")),
        };
        McpToolResult::output(result, structured)
    }
}
