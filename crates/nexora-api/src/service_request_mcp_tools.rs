use crate::{ApiState, api_error::ApiError, mcp_tool_result::McpToolResult};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct ServiceRequestMcpTools;
impl ServiceRequestMcpTools {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/service_request_tools.json"))
            .expect("request tool schema")
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let schema = Self::list()
            .into_iter()
            .find(|t| t["name"] == name)
            .ok_or((-32601, "Unknown request tool"))?;
        let obj = args
            .as_object()
            .ok_or((-32602, "Arguments must be an object"))?;
        if obj
            .keys()
            .any(|k| schema["inputSchema"]["properties"].get(k).is_none())
        {
            return Err((-32602, "Unknown request argument"));
        }
        let text = |key: &str| {
            args[key]
                .as_str()
                .map(str::to_owned)
                .ok_or((-32602, "Required string argument missing"))
        };
        let result: Result<Value, ApiError> = match name {
            "nexora_request_list" => {
                let search = args["search"].as_str().unwrap_or("").to_owned();
                let status = args["status"].as_str().unwrap_or("").to_owned();
                let page = args["page"].as_i64().unwrap_or(1);
                let type_id = args["type_id"].as_str().unwrap_or("").to_owned();
                state
                    .run(move |s| {
                        s.service_requests_filtered(&ctx, &search, &status, &type_id, page)
                    })
                    .await
            }
            "nexora_request_report" => {
                state
                    .run(move |s| s.service_request_report(&ctx, &args))
                    .await
            }
            "nexora_request_products_list" => state.run(move |s| s.products(&ctx, false)).await,
            "nexora_request_types_list" => state.run(move |s| s.service_request_types(&ctx)).await,
            "nexora_request_get" => {
                let id = text("request_id")?;
                state.run(move |s| s.service_request(&ctx, &id)).await
            }
            "nexora_request_attachment_get" => {
                let id = text("request_id")?;
                let attachment = text("attachment_id")?;
                state.run(move|s|{let(name,media,bytes)=s.service_request_attachment(&ctx,&id,&attachment)?;Ok(json!({"name":name,"media_type":media,"data_base64":STANDARD.encode(bytes)}))}).await
            }
            "nexora_request_change" | "nexora_request_link" => {
                let id = text("request_id")?;
                let key = text("idempotency_key")?;
                let mut input = args.clone();
                input.as_object_mut().unwrap().remove("request_id");
                input.as_object_mut().unwrap().remove("idempotency_key");
                let canonical = json!({"id":id,"input":input});
                if name == "nexora_request_link" {
                    state
                        .mutate(ctx, "service_request.link", key, canonical, move |s, c| {
                            s.link_service_request(c, &id, &input)
                        })
                        .await
                } else {
                    state
                        .mutate(
                            ctx,
                            "service_request.change",
                            key,
                            canonical,
                            move |s, c| s.mutate_service_request(c, Some(&id), "change", &input),
                        )
                        .await
                }
            }
            "nexora_request_create" => {
                let key = text("request_id")?;
                text("title")?;
                text("type_id")?;
                text("body")?;
                let mut input = args.clone();
                input.as_object_mut().unwrap().remove("request_id");
                let canonical = json!({"id":null,"input":input});
                state
                    .mutate(
                        ctx,
                        "service_request.create",
                        key,
                        canonical,
                        move |s, c| s.mutate_service_request(c, None, "create", &input),
                    )
                    .await
            }
            "nexora_request_comment" => {
                let id = text("request_id")?;
                let key = text("idempotency_key")?;
                let body = text("body")?;
                let input = json!({"body":body});
                let canonical = json!({"id":id,"input":input});
                state
                    .mutate(
                        ctx,
                        "service_request.comment",
                        key,
                        canonical,
                        move |s, c| s.mutate_service_request(c, Some(&id), "comment", &input),
                    )
                    .await
            }
            "nexora_request_attachment_upload" => {
                let id = text("request_id")?;
                let key = text("idempotency_key")?;
                let filename = text("name")?;
                let encoded = text("data_base64")?;
                if encoded.len() > 28 * 1024 * 1024 {
                    return McpToolResult::output(
                        Err(ApiError(PlatformError::Validation(
                            "attachment exceeds 20 MB",
                        ))),
                        structured,
                    );
                }
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| (-32602, "Invalid base64"))?;
                let digest = nexora_platform_store::ServiceRequestUploadDigest::of(&bytes);
                let canonical = json!({"id":id,"name":filename,"digest":digest});
                state
                    .mutate(
                        ctx,
                        "service_request.attach",
                        key,
                        canonical,
                        move |s, c| s.attach_service_request(c, &id, &filename, &bytes),
                    )
                    .await
            }
            _ => return Err((-32601, "Unknown request tool")),
        };
        McpToolResult::output(result, structured)
    }
}
