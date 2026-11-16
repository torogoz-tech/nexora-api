use crate::{
    ApiState, api_error::ApiError, knowledge_contribute_input::KnowledgeContributeInput,
    knowledge_list_query::KnowledgeListQuery, knowledge_page_query::KnowledgePageQuery,
    knowledge_review_input::KnowledgeReviewInput, knowledge_revise_input::KnowledgeReviseInput,
    mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, PlatformError};
use nexora_runtime::KnowledgeCommand;
use serde_json::{Value, json};

pub(crate) struct KnowledgeHandlers;
impl KnowledgeHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        query: Result<Query<KnowledgeListQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid knowledge query")))?;
        state
            .run(move |store| {
                store.query_repository_knowledge(
                    &ctx,
                    &project,
                    &repository,
                    input.snapshot.as_deref(),
                    &KnowledgeCommand::List {
                        status: input.status,
                        node: input.node,
                        offset: input.offset.unwrap_or(0),
                        limit: input.limit.unwrap_or(50),
                    },
                )
            })
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, id)): Path<(String, String, String)>,
        query: Result<Query<KnowledgePageQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid knowledge query")))?;
        state
            .run(move |store| {
                store.query_repository_knowledge(
                    &ctx,
                    &project,
                    &repository,
                    input.snapshot.as_deref(),
                    &KnowledgeCommand::Get {
                        id,
                        offset: input.offset.unwrap_or(0),
                        limit: input.limit.unwrap_or(50),
                    },
                )
            })
            .await
            .map(Json)
    }
    pub async fn pending(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        query: Result<Query<KnowledgePageQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid pending query")))?;
        state
            .run(move |store| {
                store.pending_repository_knowledge(
                    &ctx,
                    &project,
                    &repository,
                    input.snapshot.as_deref(),
                    input.offset.unwrap_or(0),
                    input.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
    pub async fn contribute(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<KnowledgeContributeInput>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid knowledge input")))?;
        let request_id = MutationId::required(&headers)?;
        let command = KnowledgeCommand::Contribute {
            request_id,
            draft: input.draft,
        };
        Self::write(
            state,
            ctx,
            project,
            repository,
            input.snapshot,
            command,
            StatusCode::CREATED,
        )
        .await
    }
    pub async fn revise(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, id)): Path<(String, String, String)>,
        headers: HeaderMap,
        body: Result<Json<KnowledgeReviseInput>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid knowledge input")))?;
        let request_id = MutationId::required(&headers)?;
        let command = KnowledgeCommand::Revise {
            request_id,
            id,
            expected_revision: input.expected_revision,
            draft: input.draft,
        };
        Self::write(
            state,
            ctx,
            project,
            repository,
            input.snapshot,
            command,
            StatusCode::OK,
        )
        .await
    }
    pub async fn review(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, id)): Path<(String, String, String)>,
        headers: HeaderMap,
        body: Result<Json<KnowledgeReviewInput>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid knowledge input")))?;
        let request_id = MutationId::required(&headers)?;
        let command = KnowledgeCommand::Review {
            request_id,
            id,
            expected_revision: input.expected_revision,
            action: input.action,
            reason: input.reason,
        };
        Self::write(
            state,
            ctx,
            project,
            repository,
            input.snapshot,
            command,
            StatusCode::OK,
        )
        .await
    }
    async fn write(
        state: ApiState,
        ctx: AuthContext,
        project: String,
        repository: String,
        snapshot: Option<String>,
        command: KnowledgeCommand,
        status: StatusCode,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let operation = match command {
            KnowledgeCommand::Contribute { .. } => "knowledge.contribute",
            KnowledgeCommand::Revise { .. } => "knowledge.revise",
            KnowledgeCommand::Review { .. } => "knowledge.review",
            _ => {
                return Err(ApiError(PlatformError::Validation(
                    "knowledge mutation required",
                )));
            }
        };
        let request_id = command
            .request_id()
            .ok_or(ApiError(PlatformError::Validation(
                "knowledge request ID required",
            )))?
            .to_owned();
        let canonical = json!({"project_id":project,"repository_id":repository,"snapshot":snapshot,"command":command});
        let body = state
            .mutate(
                ctx,
                operation,
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.query_repository_knowledge(
                        ctx,
                        &project,
                        &repository,
                        snapshot.as_deref(),
                        &command,
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status,
            request_id,
            body: Some(body),
        })
    }
}
