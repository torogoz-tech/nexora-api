use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GitAuthorHandlers;
impl GitAuthorHandlers {
    pub async fn archive(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, action)): Path<(String, String)>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let archived = match action.as_str() {
            "archive" => true,
            "restore" => false,
            _ => return Err(ApiError(PlatformError::NotFound)),
        };
        let key = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                if archived {
                    "git.author.archive"
                } else {
                    "git.author.restore"
                },
                key.clone(),
                json!({"id":id,"archived":archived}),
                move |store, ctx| store.archive_git_author(ctx, &id, archived),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: key,
            body: Some(result),
        })
    }
    pub async fn authors(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<GitAuthorQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.git_authors(&ctx, &q))
            .await
            .map(Json)
    }
    pub async fn aliases(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<GitAuthorQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.git_author_aliases(&ctx, &q))
            .await
            .map(Json)
    }
    pub async fn member_aliases(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        Query(mut q): Query<GitAuthorQuery>,
    ) -> Result<Json<Value>, ApiError> {
        q.member_id = Some(actor);
        state
            .run(move |store| store.git_author_aliases(&ctx, &q))
            .await
            .map(Json)
    }
    pub async fn options(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<GitAuthorQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.git_author_options(&ctx, q.project_id.as_deref()))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        body: Result<Json<GitAuthorAliasCreate>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid Git author alias")))?;
        let id = MutationId::required(&headers)?;
        let canonical = json!({"actor_id":actor,"alias":input});
        let value = state
            .mutate(
                ctx,
                "git.author_alias.create",
                id.clone(),
                canonical,
                move |store, ctx| store.create_git_author_alias(ctx, &actor, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id: id,
            body: Some(value),
        })
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((actor, alias)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<GitAuthorAliasRevoke>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid Git author alias")))?;
        let id = MutationId::required(&headers)?;
        let canonical = json!({"actor_id":actor,"alias_id":alias,"change":input});
        let value = state
            .mutate(
                ctx,
                "git.author_alias.revoke",
                id.clone(),
                canonical,
                move |store, ctx| store.revoke_git_author_alias(ctx, &actor, &alias, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(value),
        })
    }
}
