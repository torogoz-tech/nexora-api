use crate::{ApiState, api_error::ApiError, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct TaskWorkMcpTools;
impl TaskWorkMcpTools {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/task_work_tools.json"))
            .expect("bundled task work schema")
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_development_workflow_get"
                | "nexora_repository_development_policy_get"
                | "nexora_repository_development_policy_set"
                | "nexora_task_work_prepare"
                | "nexora_task_work_get"
                | "nexora_task_work_report"
        )
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid task work arguments");
        let result: Result<Value, ApiError> = match name {
            "nexora_development_workflow_get" => {
                let q: crate::mcp_project_query::McpProjectQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state.run(move|s|{s.get_project(&ctx,&q.project_id)?;Ok(json!({"version":"2026-11-08","skill":include_str!("../../../.agents/skills/nexora-development-workflow/SKILL.md"),"installation_guide":include_str!("../../../docs/platform/development-agents.md"),"task_pool_guide":include_str!("../../../docs/platform/task-pool.md")}))}).await
            }
            "nexora_repository_development_policy_get" => {
                let q: crate::mcp_repository_query::McpRepositoryQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |s| {
                        serde_json::to_value(s.development_policy(
                            &ctx,
                            &q.project_id,
                            &q.repository_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_repository_development_policy_set" => {
                let q: crate::mcp_development_policy_set::McpDevelopmentPolicySet =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":q.project_id,"repository_id":q.repository_id,"policy":q.policy});
                state
                    .mutate(
                        ctx,
                        "development_policy.change",
                        q.request_id,
                        canonical,
                        move |s, c| {
                            serde_json::to_value(s.change_development_policy(
                                c,
                                &q.project_id,
                                &q.repository_id,
                                &q.policy,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_work_prepare" => {
                let q: crate::mcp_task_work_prepare::McpTaskWorkPrepare =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":q.project_id,"task_id":q.task_id,"preparation":q.preparation});
                state
                    .mutate(
                        ctx,
                        "task.work.prepare",
                        q.request_id,
                        canonical,
                        move |s, c| {
                            serde_json::to_value(s.prepare_task_work(
                                c,
                                &q.project_id,
                                &q.task_id,
                                &q.preparation,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_work_report" => {
                let q: crate::mcp_task_work_report::McpTaskWorkReport =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":q.project_id,"task_id":q.task_id,"session_id":q.session_id,"report":q.report});
                state
                    .mutate(
                        ctx,
                        "task.work.report",
                        q.request_id,
                        canonical,
                        move |s, c| {
                            serde_json::to_value(s.report_task_work(
                                c,
                                &q.project_id,
                                &q.task_id,
                                &q.session_id,
                                &q.report,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_work_get" => {
                let q: crate::mcp_task_work_query::McpTaskWorkQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |s| {
                        serde_json::to_value(s.task_work(
                            &ctx,
                            &q.project_id,
                            &q.task_id,
                            &q.session_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown task work tool")),
        };
        McpToolResult::output(result, structured)
    }
}
