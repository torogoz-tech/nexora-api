use crate::{
    ApiState, mcp_task_template_apply::McpTaskTemplateApply,
    mcp_task_template_change::McpTaskTemplateChange,
    mcp_task_template_create::McpTaskTemplateCreate, mcp_task_template_get::McpTaskTemplateGet,
    mcp_task_template_list::McpTaskTemplateList, mcp_task_template_publish::McpTaskTemplatePublish,
    mcp_task_template_version_get::McpTaskTemplateVersionGet,
    mcp_task_template_versions_list::McpTaskTemplateVersionsList, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct TaskTemplateMcpTools;
impl TaskTemplateMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid template tool arguments");
        let result = match name {
            "nexora_task_template_create" => {
                let input: McpTaskTemplateCreate = serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "task_template.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_task_template(
                                ctx,
                                &input.project_id,
                                &input.definition,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_template_change" => {
                let input: McpTaskTemplateChange = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"template_id":input.template_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "task_template.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_task_template(
                                ctx,
                                &input.project_id,
                                &input.template_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_template_publish" => {
                let input: McpTaskTemplatePublish =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"template_id":input.template_id,"publication":input.publication});
                state
                    .mutate(
                        ctx,
                        "task_template.publish",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.publish_task_template_version(
                                ctx,
                                &input.project_id,
                                &input.template_id,
                                &input.publication,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_template_apply" => {
                let input: McpTaskTemplateApply = serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"application":input.application});
                state
                    .mutate(
                        ctx,
                        "task.template_apply",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.apply_task_template(
                                ctx,
                                &input.project_id,
                                &input.application,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_template_get" => {
                let input: McpTaskTemplateGet = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_task_template(
                            &ctx,
                            &input.project_id,
                            &input.template_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_template_version_get" => {
                let input: McpTaskTemplateVersionGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.task_template_version(
                            &ctx,
                            &input.project_id,
                            &input.template_id,
                            &input.version_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_templates_list" => {
                let input: McpTaskTemplateList = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.task_template_page(
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
            "nexora_task_template_versions_list" => {
                let input: McpTaskTemplateVersionsList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.task_template_version_page(
                            &ctx,
                            &input.project_id,
                            &input.template_id,
                            input.before,
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown template tool")),
        };
        McpToolResult::output(result, structured)
    }
}
