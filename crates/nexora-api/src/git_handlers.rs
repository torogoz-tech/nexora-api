use crate::{
    ApiState, api_error::ApiError, git_callback_query::GitCallbackQuery,
    git_start_request::GitStartRequest, mutation_id::MutationId,
    mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct GitHandlers;
impl GitHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<
            Query<crate::git_connection_list_query::GitConnectionListQuery>,
            QueryRejection,
        >,
    ) -> Result<Json<GitConnectionPage>, ApiError> {
        let Query(page) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid connection query")))?;
        Ok(Json(
            state
                .run(move |s| {
                    s.git_connection_page(
                        &ctx,
                        None,
                        page.after.as_deref(),
                        page.limit.unwrap_or(50),
                    )
                })
                .await?,
        ))
    }
    pub async fn project_connections(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        query: Result<
            Query<crate::git_connection_list_query::GitConnectionListQuery>,
            QueryRejection,
        >,
    ) -> Result<Json<GitConnectionPage>, ApiError> {
        let Query(page) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid connection query")))?;
        Ok(Json(
            state
                .run(move |s| {
                    s.git_connection_page(
                        &ctx,
                        Some(&project),
                        page.after.as_deref(),
                        page.limit.unwrap_or(50),
                    )
                })
                .await?,
        ))
    }
    pub async fn configuration(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<GitlabConfiguration>, ApiError> {
        Ok(Json(
            state.run(move |s| s.gitlab_configuration(&ctx)).await?,
        ))
    }
    pub async fn change_state(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<GitConfigurationStateChange>, JsonRejection>,
    ) -> Result<MutationResponse<GitlabConfiguration>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid provider state change")))?;
        let policy = state.git_policy();
        let request_id = MutationId::read(&headers)?;
        let data = serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let result = state
            .mutate(
                ctx,
                "gitlab.configuration.state",
                request_id.clone(),
                data,
                move |store, ctx| store.change_gitlab_configuration_state(ctx, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(result),
        })
    }
    pub async fn configure(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<GitlabClientRegistration>, JsonRejection>,
    ) -> Result<MutationResponse<GitlabConfiguration>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid GitLab configuration")))?;
        let status = if input.expected_revision == 0 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        let policy = state.git_policy();
        let request_id = MutationId::read(&headers)?;
        let data = serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let config = state
            .mutate(
                ctx,
                "gitlab.configure",
                request_id.clone(),
                data,
                move |s, c| s.configure_gitlab(c, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status,
            request_id,
            body: Some(config),
        })
    }
    pub async fn start(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        body: Result<Json<GitStartRequest>, JsonRejection>,
    ) -> Result<Response, ApiError> {
        let _ = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "Git start expects an empty object",
            ))
        })?;
        let policy = state.git_policy();
        let flow = state.run(move |s| s.start_gitlab(&ctx, &policy)).await?;
        let cookie = format!(
            "nexora_git_{}={}; HttpOnly; SameSite=Lax; Path=/api/v1/git/gitlab/callback; Max-Age=600{}",
            flow.state_id,
            flow.correlation_secret,
            if flow.secure_cookie { "; Secure" } else { "" }
        );
        let mut response = Json(flow).into_response();
        response.headers_mut().insert(
            "Set-Cookie",
            HeaderValue::from_str(&cookie).map_err(|_| ApiError(PlatformError::Storage))?,
        );
        Ok(response)
    }
    pub async fn callback(
        State(state): State<ApiState>,
        headers: HeaderMap,
        query: Result<Query<GitCallbackQuery>, QueryRejection>,
    ) -> Response {
        let Ok(Query(input)) = query else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        let Some(token) = input.state else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        let Some((id, _)) = token.split_once('.') else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        if id.len() != 36
            || !id.starts_with("gsa_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let name = format!("nexora_git_{id}");
        let values: Vec<_> = headers
            .get_all("cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| s.trim().split_once('='))
            .filter(|(k, _)| *k == name)
            .map(|(_, v)| v.to_owned())
            .collect();
        if values.len() != 1 {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let cookie = values.into_iter().next().unwrap();
        let policy = state.git_policy();
        let code = if input.error.is_some() {
            String::new()
        } else {
            input.code.unwrap_or_default()
        };
        let mut response = match state
            .run(move |s| s.complete_gitlab(&token, &code, &cookie, &policy))
            .await
        {
            Ok(connection) => Json(connection).into_response(),
            Err(error) => error.into_response(),
        };
        if let Ok(value) = HeaderValue::from_str(&format!(
            "{name}=; HttpOnly; SameSite=Lax; Path=/api/v1/git/gitlab/callback; Max-Age=0"
        )) {
            response.headers_mut().insert("Set-Cookie", value);
        }
        response
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request_id = MutationId::read(&headers)?;
        state
            .mutate(
                ctx,
                "git.connection.revoke",
                request_id.clone(),
                json!({"connection_id":id}),
                move |s, c| s.revoke_git_connection(c, &id),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
    pub async fn share(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, project)): Path<(String, String)>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request_id = MutationId::read(&headers)?;
        state
            .mutate(
                ctx,
                "git.connection.share",
                request_id.clone(),
                json!({"connection_id":id,"project_id":project}),
                move |s, c| s.share_git_connection(c, &id, &project),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
    pub async fn unshare(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, project)): Path<(String, String)>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request_id = MutationId::read(&headers)?;
        state
            .mutate(
                ctx,
                "git.connection.unshare",
                request_id.clone(),
                json!({"connection_id":id,"project_id":project}),
                move |s, c| s.unshare_git_connection(c, &id, &project),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
    pub async fn authorize(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repo)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<GitRepositoryAuthorization>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectRepository>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid repository authorization",
            ))
        })?;
        let request_id = MutationId::read(&headers)?;
        let policy = state.git_policy();
        let data = json!({"project_id":project,"repository_id":repo,"authorization":input});
        let repository = state
            .mutate(
                ctx,
                "repository.authorize",
                request_id.clone(),
                data,
                move |s, c| s.authorize_repository(c, &project, &repo, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(repository),
        })
    }
}
