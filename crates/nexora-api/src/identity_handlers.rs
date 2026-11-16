use crate::{
    ApiState, api_error::ApiError, create_actor_request::CreateActorRequest,
    create_user_request::CreateUserRequest, login_request::LoginRequest, mutation_id::MutationId,
    mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{Actor, AuthContext, PlatformError};
use serde_json::json;
pub(crate) struct IdentityHandlers;
impl IdentityHandlers {
    pub async fn login(
        State(state): State<ApiState>,
        headers: HeaderMap,
        body: Result<Json<LoginRequest>, JsonRejection>,
    ) -> Result<axum::response::Response, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid login request")))?;
        crate::browser_session::BrowserSession::origin(&headers)?;
        let session = state
            .run(move |store| store.login(&input.username, &input.password))
            .await?;
        crate::browser_session::BrowserSession::issued(session, &headers)
    }
    pub async fn logout(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
    ) -> Result<StatusCode, ApiError> {
        if ctx.method() != nexora_platform::AuthMethod::Session {
            return Err(ApiError(PlatformError::Forbidden));
        }
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(ApiError(PlatformError::Unauthorized))?
            .to_owned();
        state.run(move |store| store.logout(&token)).await?;
        Ok(StatusCode::NO_CONTENT)
    }
    pub async fn create_actor(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<CreateActorRequest>, JsonRejection>,
    ) -> Result<MutationResponse<Actor>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid actor request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = serde_json::to_value(&input)
            .map_err(|_| ApiError(PlatformError::Validation("invalid actor request")))?;
        let actor = state
            .mutate(
                ctx,
                "actor.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_actor(ctx, input.kind, &input.name, &input.purpose),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(actor),
        })
    }
    pub async fn create_user(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<CreateUserRequest>, JsonRejection>,
    ) -> Result<MutationResponse<Actor>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid user request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = serde_json::to_value(&input)
            .map_err(|_| ApiError(PlatformError::Validation("invalid user request")))?;
        let actor = state
            .mutate(
                ctx,
                "user.create",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.create_user(ctx, &input.username, &input.password, &input.name)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(actor),
        })
    }
    pub async fn suspend_actor(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":id});
        state
            .mutate(
                ctx,
                "actor.suspend",
                request_id.clone(),
                canonical,
                move |store, ctx| store.suspend_actor(ctx, &id),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
}
