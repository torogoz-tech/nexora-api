use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct DocumentMcpTools;
impl DocumentMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid document tool arguments");
        let result = match name {
            "nexora_document_create" => {
                let input: crate::mcp_document_create::McpDocumentCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "document.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_project_document(
                                ctx,
                                &input.project_id,
                                &input.definition,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_change" => {
                let input: crate::mcp_document_change::McpDocumentChange =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"document_id":input.document_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "document.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_project_document(
                                ctx,
                                &input.project_id,
                                &input.document_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_revise" => {
                let input: crate::mcp_document_revise::McpDocumentRevise =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"document_id":input.document_id,"revision":input.revision});
                state
                    .mutate(
                        ctx,
                        "document.revise",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.revise_project_document(
                                ctx,
                                &input.project_id,
                                &input.document_id,
                                &input.revision,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_transition" => {
                let input: crate::mcp_document_transition::McpDocumentTransition =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"document_id":input.document_id,"version_id":input.version_id,"transition_id":input.transition_id,"transition":input.transition});
                state
                    .mutate(
                        ctx,
                        "document.transition",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.transition_document_version(
                                ctx,
                                &input.project_id,
                                &input.document_id,
                                &input.version_id,
                                &input.transition_id,
                                &input.transition,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_publish" => {
                let input: crate::mcp_document_publish::McpDocumentPublish =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"document_id":input.document_id,"publication":input.publication});
                state
                    .mutate(
                        ctx,
                        "document.publish",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.publish_project_document(
                                ctx,
                                &input.project_id,
                                &input.document_id,
                                &input.publication,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_document_get" => {
                let input: crate::mcp_document_get::McpDocumentGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_project_document(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_document_version_get" => {
                let input: crate::mcp_document_version_get::McpDocumentVersionGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.document_version(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                            &input.version_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_document_versions_list" => {
                let input: crate::mcp_document_versions_list::McpDocumentVersionsList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.document_version_page(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                            input.before,
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_document_export" => {
                let input: crate::mcp_document_export::McpDocumentExport =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.export_project_document(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                            &input.version_id,
                            &input.format,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_documents_list" => {
                let input: crate::mcp_document_list::McpDocumentList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.project_document_page(
                            &ctx,
                            &input.project_id,
                            &input.query,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_document_history" => {
                let input: crate::mcp_document_history::McpDocumentHistory =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.document_history_page(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                            input.after,
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_document_publications_list" => {
                let input: crate::mcp_document_publications_list::McpDocumentPublicationsList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.document_publication_page(
                            &ctx,
                            &input.project_id,
                            &input.document_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown document tool")),
        };
        McpToolResult::output(result, structured)
    }
}
