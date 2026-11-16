use crate::{ApiState, api_error::ApiError, mutation_id::MutationId};
use axum::{Extension, Json, extract::State, http::HeaderMap};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct ProductHandlers;
impl ProductHandlers {
    pub async fn list(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(s.run(move |store| store.products(&ctx, true)).await?))
    }
    pub async fn public_list(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(s.run(move |store| store.products(&ctx, false)).await?))
    }
    pub async fn save(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            s.mutate(
                ctx,
                "product.save",
                MutationId::required(&h)?,
                v.clone(),
                move |store, c| store.save_product(c, &v),
            )
            .await?,
        ))
    }
}
