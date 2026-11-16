use crate::{
    ApiState, api_error::ApiError, member_avatar_input::MemberAvatarInput, mutation_id::MutationId,
    mutation_response::MutationResponse, organization_logo::OrganizationLogo,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct MemberAvatarHandlers;
impl MemberAvatarHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<impl IntoResponse, ApiError> {
        let image = state
            .run(move |store| store.member_avatar(&ctx, &id))
            .await?;
        Ok((
            [
                ("content-type", "image/png"),
                ("x-content-type-options", "nosniff"),
                ("cache-control", "private, no-store"),
            ],
            image,
        ))
    }
    pub async fn put(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<MemberAvatarInput>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid avatar request")))?;
        let id = MutationId::required(&headers)?;
        let fingerprint =
            json!({"image_digest":format!("{:x}",Sha256::digest(input.image_base64.as_bytes()))});
        let result=state.mutate(ctx,"member.avatar.change",id.clone(),fingerprint,move|store,ctx|{
            store.authorize_member_avatar_change(ctx)?;
            let image=OrganizationLogo::normalize(&input.image_base64).map_err(|error|match error { PlatformError::Validation(_)=>PlatformError::Validation("avatar must be a valid PNG, JPEG or WebP up to 2 MB and 4096 pixels"),other=>other })?;
            store.change_member_avatar(ctx,Some(image))
        }).await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn delete(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "member.avatar.delete",
                id.clone(),
                json!({}),
                move |store, ctx| store.change_member_avatar(ctx, None),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
