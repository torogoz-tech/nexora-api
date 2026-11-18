use crate::{ApiState, api_error::ApiError, mutation_id::MutationId};
use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, header},
    response::IntoResponse,
};
use nexora_platform::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(crate) struct ServiceRequestHandlers;
impl ServiceRequestHandlers {
    pub async fn report(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<BTreeMap<String, String>>,
    ) -> Result<axum::response::Response, ApiError> {
        let csv = q.get("format").is_some_and(|v| v == "csv");
        let mut filters = serde_json::to_value(&q).unwrap();
        let page = q
            .get("page")
            .map(|s| s.parse::<i64>())
            .transpose()
            .map_err(|_| ApiError(PlatformError::Validation("invalid page")))?
            .unwrap_or(1);
        filters["page"] = json!(page);
        filters["export"] = json!(csv);
        let result = s
            .run(move |store| store.service_request_report(&ctx, &filters))
            .await?;
        if csv {
            Ok((
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=solicitudes.csv",
                    ),
                ],
                nexora_platform_store::PlatformStore::service_request_report_csv(&result),
            )
                .into_response())
        } else {
            Ok(Json(result).into_response())
        }
    }

    pub async fn list(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<BTreeMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let search = q.get("search").cloned().unwrap_or_default();
        let status = q.get("status").cloned().unwrap_or_default();
        let type_id = q.get("type_id").cloned().unwrap_or_default();
        let product_id = q.get("product_id").cloned().unwrap_or_default();
        let criticality = q.get("criticality").cloned().unwrap_or_default();
        let order = q.get("order").cloned();
        let page = q
            .get("page")
            .map(|v| v.parse::<i64>())
            .transpose()
            .map_err(|_| ApiError(PlatformError::Validation("invalid page")))?
            .unwrap_or(1);
        Ok(Json(
            s.run(move |store| {
                store.service_requests_ordered_with_criticality(
                    &ctx,
                    &search,
                    &status,
                    &type_id,
                    &product_id,
                    &criticality,
                    page,
                    order.as_deref(),
                )
            })
            .await?,
        ))
    }
    pub async fn get(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            s.run(move |store| store.service_request(&ctx, &id)).await?,
        ))
    }
    pub async fn types(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            s.run(move |store| store.service_request_types(&ctx))
                .await?,
        ))
    }
    pub async fn create(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        Self::mutate(s, ctx, h, None, "create", v).await
    }
    pub async fn change(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        Self::mutate(s, ctx, h, Some(id), "change", v).await
    }
    pub async fn comment(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        Self::mutate(s, ctx, h, Some(id), "comment", v).await
    }
    async fn mutate(
        s: ApiState,
        ctx: AuthContext,
        h: HeaderMap,
        id: Option<String>,
        action: &'static str,
        v: Value,
    ) -> Result<Json<Value>, ApiError> {
        let key = MutationId::required(&h)?;
        let canonical = json!({"id":id,"input":v});
        Ok(Json(
            s.mutate(
                ctx,
                match action {
                    "create" => "service_request.create",
                    "change" => "service_request.change",
                    _ => "service_request.comment",
                },
                key,
                canonical,
                move |store, ctx| store.mutate_service_request(ctx, id.as_deref(), action, &v),
            )
            .await?,
        ))
    }
    pub async fn save_type(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        let key = MutationId::required(&h)?;
        Ok(Json(
            s.mutate(
                ctx,
                "service_request.type",
                key,
                v.clone(),
                move |store, ctx| store.save_service_request_type(ctx, &v),
            )
            .await?,
        ))
    }
    pub async fn link(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        let key = MutationId::required(&h)?;
        let canonical = json!({"id":id,"input":v});
        Ok(Json(
            s.mutate(
                ctx,
                "service_request.link",
                key,
                canonical,
                move |store, ctx| store.link_service_request(ctx, &id, &v),
            )
            .await?,
        ))
    }
    pub async fn linked(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<BTreeMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let kind = q.get("kind").cloned().unwrap_or_default();
        let target = q.get("target_id").cloned().unwrap_or_default();
        Ok(Json(
            s.run(move |store| store.linked_service_requests(&ctx, &kind, &target))
                .await?,
        ))
    }
    pub async fn upload(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Query(q): Query<BTreeMap<String, String>>,
        body: Bytes,
    ) -> Result<Json<Value>, ApiError> {
        let key = MutationId::required(&h)?;
        let name = q.get("name").cloned().unwrap_or_default();
        let digest = nexora_platform_store::ServiceRequestUploadDigest::of(&body);
        let canonical = json!({"id":id,"name":name,"digest":digest});
        Ok(Json(
            s.mutate(
                ctx,
                "service_request.attach",
                key,
                canonical,
                move |store, ctx| store.attach_service_request(ctx, &id, &name, &body),
            )
            .await?,
        ))
    }
    pub async fn content(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, attachment)): Path<(String, String)>,
    ) -> Result<impl IntoResponse, ApiError> {
        let (name, media, bytes) = s
            .run(move |store| store.service_request_attachment(&ctx, &id, &attachment))
            .await?;
        let filename: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect();
        Ok((
            [
                (header::CONTENT_TYPE, media),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                ),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "sandbox; default-src 'none'".into(),
                ),
            ],
            bytes,
        ))
    }
}
