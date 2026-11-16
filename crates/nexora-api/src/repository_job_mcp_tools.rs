use crate::{
    ApiState, mcp_repository_job_cancel::McpRepositoryJobCancel,
    mcp_repository_job_query::McpRepositoryJobQuery,
    mcp_repository_job_request::McpRepositoryJobRequest, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError, RepositoryJobKind};
use serde_json::{Value, json};

pub(crate) struct RepositoryJobMcpTools;
impl RepositoryJobMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid registered repository job arguments");
        let result = match name {
            "nexora_sync_repository" | "nexora_index_repository" => {
                let input: McpRepositoryJobRequest =
                    serde_json::from_value(args).map_err(invalid)?;
                Self::ids(&input.project_id, &input.repository_id)?;
                let kind = if name == "nexora_sync_repository" {
                    RepositoryJobKind::Sync
                } else {
                    RepositoryJobKind::Index
                };
                let canonical = json!({"project_id":input.project_id,"repository_id":input.repository_id,"request":input.request});
                state
                    .mutate(
                        ctx,
                        kind.operation(),
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.enqueue_repository_job(
                                ctx,
                                &input.project_id,
                                &input.repository_id,
                                kind,
                                &input.request,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_repository_job_get" => {
                let input: McpRepositoryJobQuery = serde_json::from_value(args).map_err(invalid)?;
                Self::ids(&input.project_id, &input.job_id)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.repository_job(
                            &ctx,
                            &input.project_id,
                            &input.job_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_repository_job_cancel" => {
                let input: McpRepositoryJobCancel =
                    serde_json::from_value(args).map_err(invalid)?;
                Self::ids(&input.project_id, &input.job_id)?;
                let canonical = json!({"project_id":input.project_id,"job_id":input.job_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "repository.job.cancel",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.cancel_repository_job(
                                ctx,
                                &input.project_id,
                                &input.job_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            _ => return Err((-32602, "Unknown registered repository job tool")),
        };
        McpToolResult::output(result, structured)
    }
    fn ids(project: &str, id: &str) -> Result<(), (i64, &'static str)> {
        if project.is_empty() || project.len() > 128 || id.is_empty() || id.len() > 128 {
            return Err((-32602, "Invalid registered repository job identifiers"));
        }
        Ok(())
    }
}
