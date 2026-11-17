use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct MemberEmailHandlers;
impl MemberEmailHandlers {
    pub async fn requests(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state.run(move |s| s.member_requests(&ctx)).await.map(Json)
    }
    pub async fn team_members(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.team_members(&ctx))
            .await
            .map(Json)
    }

    pub async fn members(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.organization_members(&ctx))
            .await
            .map(Json)
    }
    pub async fn invitations(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Vec<MemberInvitation>>, ApiError> {
        state
            .run(move |s| s.member_invitations(&ctx))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<MemberInvitationCreate>, JsonRejection>,
    ) -> Result<MutationResponse<MemberInvitation>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid invitation request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "member.invite",
                id.clone(),
                json!(input),
                move |s, c| s.create_member_invitation(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn resend(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<MemberInvitationChange>, JsonRejection>,
    ) -> Result<MutationResponse<MemberInvitation>, ApiError> {
        Self::change(state, ctx, id, headers, body, true).await
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<MemberInvitationChange>, JsonRejection>,
    ) -> Result<MutationResponse<MemberInvitation>, ApiError> {
        Self::change(state, ctx, id, headers, body, false).await
    }
    async fn change(
        state: ApiState,
        ctx: AuthContext,
        id: String,
        headers: HeaderMap,
        body: Result<Json<MemberInvitationChange>, JsonRejection>,
        resend: bool,
    ) -> Result<MutationResponse<MemberInvitation>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid invitation request")))?;
        let key = MutationId::required(&headers)?;
        let operation = if resend {
            "member.invitation.resend"
        } else {
            "member.invitation.revoke"
        };
        let result = state
            .mutate(
                ctx,
                operation,
                key.clone(),
                json!({"id":id,"input":input}),
                move |s, c| s.change_member_invitation(c, &id, &input, resend),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: key,
            body: Some(result),
        })
    }
}
