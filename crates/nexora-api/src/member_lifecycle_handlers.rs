use crate::{ApiState, api_error::ApiError, member_lifecycle_request::MemberLifecycleRequest};
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct MemberLifecycleHandlers;
impl MemberLifecycleHandlers {
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        Json(input): Json<MemberLifecycleRequest>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| {
                store.change_member_archived(&ctx, &actor, input.archived, input.expected_revision)
            })
            .await
            .map(Json)
    }
}
