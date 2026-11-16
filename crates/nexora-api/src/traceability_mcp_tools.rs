use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct TraceabilityMcpTools;
impl TraceabilityMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid traceability tool arguments");
        let result = match name {
            "nexora_document_link_create" => {
                let input: crate::mcp_document_link_create::McpDocumentLinkCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"document_id":input.document_id,"link":input.link});
                state
                    .mutate(
                        ctx,
                        "document.link_create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_document_link(
                                ctx,
                                &input.project_id,
                                &input.document_id,
                                &input.link,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_link_remove" => {
                let input: crate::mcp_document_link_remove::McpDocumentLinkRemove =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"document_id":input.document_id,"link_id":input.link_id,"removal":input.removal});
                state
                    .mutate(
                        ctx,
                        "document.link_remove",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.remove_document_link(
                                ctx,
                                &input.project_id,
                                &input.document_id,
                                &input.link_id,
                                &input.removal,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_context_package_create" => {
                let input: crate::mcp_context_package_create::McpContextPackageCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"selection":input.selection});
                state
                    .mutate(
                        ctx,
                        "context_package.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_project_context_package(
                                ctx,
                                &input.project_id,
                                &input.selection,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_links_list" => {
                let input: crate::mcp_document_links_list::McpDocumentLinksList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.document_links_page(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                            input.include_removed.unwrap_or(false),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_documents_list" => {
                let input: crate::mcp_task_documents_list::McpTaskDocumentsList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.task_document_links_page(
                            &ctx,
                            &input.project_id,
                            &input.task_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                            input.include_removed.unwrap_or(false),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_context_package_get" => {
                let input: crate::mcp_context_package_get::McpContextPackageGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_project_context_package(
                            &ctx,
                            &input.project_id,
                            &input.package_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_context_package_export" => {
                let input: crate::mcp_context_package_export::McpContextPackageExport =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.export_project_context_package(
                            &ctx,
                            &input.project_id,
                            &input.package_id,
                            &input.format,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown traceability tool")),
        };
        McpToolResult::output(result, structured)
    }
}
