use crate::{
    ApiState, mcp_repository_change::McpRepositoryChange,
    mcp_repository_list_query::McpRepositoryListQuery, mcp_repository_query::McpRepositoryQuery,
    mcp_repository_registration::McpRepositoryRegistration, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct RepositoryMcpTools;
impl RepositoryMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let result = match name {
            "nexora_repositories_list" => {
                let input: McpRepositoryListQuery = serde_json::from_value(args)
                    .map_err(|_| (-32602, "Invalid repository list arguments"))?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.repository_page(
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
            "nexora_repository_get" => {
                let input: McpRepositoryQuery = serde_json::from_value(args)
                    .map_err(|_| (-32602, "Invalid repository lookup arguments"))?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_repository(
                            &ctx,
                            &input.project_id,
                            &input.repository_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_repository_register" => {
                let input: McpRepositoryRegistration = serde_json::from_value(args)
                    .map_err(|_| (-32602, "Invalid repository registration arguments"))?;
                let canonical =
                    json!({"project_id":input.project_id,"registration":input.registration});
                state
                    .mutate(
                        ctx,
                        "repository.register",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.register_repository(
                                ctx,
                                &input.project_id,
                                &input.registration,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_repository_change" => {
                let input: McpRepositoryChange = serde_json::from_value(args)
                    .map_err(|_| (-32602, "Invalid repository change arguments"))?;
                let canonical = json!({"project_id":input.project_id,"repository_id":input.repository_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "repository.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_repository(
                                ctx,
                                &input.project_id,
                                &input.repository_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            _ => return Err((-32602, "Unknown repository tool name")),
        };
        McpToolResult::output(result, structured)
    }
}
