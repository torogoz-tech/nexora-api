use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct WorkCatalogMcpTools;
impl WorkCatalogMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid work catalog arguments");
        let result = match name {
            "nexora_task_type_change" => {
                let input: crate::mcp_task_type_change::McpTaskTypeChange =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"type_code":input.type_code,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "task_type.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_task_type(
                                ctx,
                                &input.project_id,
                                &input.type_code,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_types_list" => {
                let input: crate::mcp_work_catalog_page::McpWorkCatalogPage =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.task_type_page(
                            &ctx,
                            &input.project_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                            input.include_archived.unwrap_or(false),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_backlogs_list" => {
                let input: crate::mcp_work_catalog_page::McpWorkCatalogPage =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.backlog_page(
                            &ctx,
                            &input.project_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                            input.include_archived.unwrap_or(false),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown work catalog tool")),
        };
        McpToolResult::output(result, structured)
    }
}
