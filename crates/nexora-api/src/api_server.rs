use crate::git_handlers::GitHandlers;
use crate::git_webhook_handlers::GitWebhookHandlers;
use crate::github_handlers::GithubHandlers;
use crate::github_manifest_handlers::GithubManifestHandlers;
use crate::oidc_handlers::OidcHandlers;
use crate::remote_repository_handlers::RemoteRepositoryHandlers;
use crate::repository_handlers::RepositoryHandlers;
use crate::repository_job_handlers::RepositoryJobHandlers;
use crate::{
    ApiState, api_error::ApiError, api_middleware::ApiMiddleware, grant_handlers::GrantHandlers,
    identity_handlers::IdentityHandlers, key_handlers::KeyHandlers,
    platform_mcp_transport::PlatformMcpTransport, project_handlers::ProjectHandlers,
};
use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::State,
    http::StatusCode,
    middleware,
    routing::{delete, get, post},
};
use nexora_platform::PlatformError;
use serde_json::json;
pub struct ApiServer;
impl ApiServer {
    pub fn router(state: ApiState) -> Router {
        let secured = Router::new()
.route("/api/v1/developer-analyses/members",get(crate::daily_developer_analysis_handlers::DailyDeveloperAnalysisHandlers::directory))
.route("/api/v1/developer-analyses",get(crate::daily_developer_analysis_handlers::DailyDeveloperAnalysisHandlers::get))
.route("/api/v1/notifications",get(crate::in_app_notification_handlers::InAppNotificationHandlers::list))
.route("/api/v1/notifications/read-all",post(crate::in_app_notification_handlers::InAppNotificationHandlers::read_all))
.route("/api/v1/notifications/{id}",axum::routing::patch(crate::in_app_notification_handlers::InAppNotificationHandlers::change))
.route("/api/v1/settings/timezone",get(crate::installation_timezone_handlers::InstallationTimezoneHandlers::get).patch(crate::installation_timezone_handlers::InstallationTimezoneHandlers::change))
.route("/api/v1/overtime",get(crate::overtime_handlers::OvertimeHandlers::list).post(crate::overtime_handlers::OvertimeHandlers::create))
.route("/api/v1/overtime/{id}/decision",post(crate::overtime_handlers::OvertimeHandlers::decide))
.route("/api/v1/overtime/evidence-candidates",get(crate::overtime_evidence_handlers::OvertimeEvidenceHandlers::planned))
.route("/api/v1/overtime/{id}/evidence-candidates",get(crate::overtime_evidence_handlers::OvertimeEvidenceHandlers::candidates))
.route("/api/v1/overtime/{id}/evidence",post(crate::overtime_evidence_handlers::OvertimeEvidenceHandlers::append))
.route("/api/v1/settings/labor-calendar",get(crate::labor_calendar_handlers::LaborCalendarHandlers::settings).put(crate::labor_calendar_handlers::LaborCalendarHandlers::change_settings))
.route("/api/v1/calendar/activities",get(crate::calendar_activity_handlers::CalendarActivityHandlers::list).post(crate::calendar_activity_handlers::CalendarActivityHandlers::create))
.route("/api/v1/calendar/activities/{id}",axum::routing::put(crate::calendar_activity_handlers::CalendarActivityHandlers::change).get(crate::calendar_activity_handlers::CalendarActivityHandlers::get))
.route("/api/v1/calendar/activities/{id}/attendance",axum::routing::put(crate::calendar_activity_handlers::CalendarActivityHandlers::attendance))
.route("/api/v1/calendar/activities/{id}/cancel",post(crate::calendar_activity_handlers::CalendarActivityHandlers::cancel))
.route("/api/v1/labor-calendar",get(crate::labor_calendar_handlers::LaborCalendarHandlers::calendar))
.route("/api/v1/labor-calendar/requests",post(crate::labor_calendar_handlers::LaborCalendarHandlers::create_request))
.route("/api/v1/labor-calendar/holidays",post(crate::labor_calendar_handlers::LaborCalendarHandlers::propose_holiday))
.route("/api/v1/labor-calendar/requests/{id}/decision",post(crate::labor_calendar_handlers::LaborCalendarHandlers::decide_request))
.route("/api/v1/labor-calendar/holidays/{id}/decision",post(crate::labor_calendar_handlers::LaborCalendarHandlers::decide_holiday))
.route("/api/v1/settings/labor-calendar/members/{id}/approver",axum::routing::put(crate::labor_calendar_handlers::LaborCalendarHandlers::assign_approver))
.route("/api/v1/members/{id}/ratings",get(crate::developer_rating_handlers::DeveloperRatingHandlers::get).post(crate::developer_rating_handlers::DeveloperRatingHandlers::create))
.route("/api/v1/me/weekly-rating", get(crate::developer_rating_handlers::DeveloperRatingHandlers::own_weekly))
.route("/api/v1/me/rating-feedback",get(crate::rating_feedback_handlers::RatingFeedbackHandlers::get).post(crate::rating_feedback_handlers::RatingFeedbackHandlers::generate))
.route("/api/v1/ratings",get(crate::developer_rating_handlers::DeveloperRatingHandlers::directory))
.route("/api/v1/settings/email-access",get(crate::email_access_handlers::EmailAccessHandlers::get).put(crate::email_access_handlers::EmailAccessHandlers::change))
.route("/api/v1/projects/{project_id}/features",get(crate::feature_handlers::FeatureHandlers::catalog))
.route("/api/v1/projects/{project_id}/features/{id}/complete",post(crate::feature_handlers::FeatureHandlers::complete))
.route("/api/v1/projects/{project_id}/feature-proposals",post(crate::feature_handlers::FeatureHandlers::propose))
.route("/api/v1/projects/{project_id}/feature-proposals/{id}",get(crate::feature_handlers::FeatureHandlers::proposal))
.route("/api/v1/projects/{project_id}/feature-proposals/{id}/decision",post(crate::feature_handlers::FeatureHandlers::decide))
.route("/api/v1/me/avatar",axum::routing::put(crate::member_avatar_handlers::MemberAvatarHandlers::put).delete(crate::member_avatar_handlers::MemberAvatarHandlers::delete).layer(DefaultBodyLimit::max(3*1024*1024)))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/branch-protection",get(crate::branch_protection_handlers::BranchProtectionHandlers::get))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/branch-protection/preview",post(crate::branch_protection_handlers::BranchProtectionHandlers::preview))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/branch-protection/apply",post(crate::branch_protection_handlers::BranchProtectionHandlers::apply))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/commits/{sha}",get(crate::repository_commit_handlers::RepositoryCommitHandlers::get))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/file",get(crate::repository_file_handlers::RepositoryFileHandlers::get))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/profile",get(crate::repository_profile_handlers::RepositoryProfileHandlers::registered))
.route("/api/v1/git/connections/{connection_id}/repositories/{remote_id}/profile",get(crate::repository_profile_handlers::RepositoryProfileHandlers::remote))
.route("/api/v1/members/{id}/avatar",get(crate::member_avatar_handlers::MemberAvatarHandlers::get))
.route("/api/v1/projects/{project_id}/profile",get(crate::project_profile_handlers::ProjectProfileHandlers::get))
.route("/api/v1/members/{id}/profile",get(crate::member_profile_handlers::MemberProfileHandlers::get))
.route("/api/v1/members/{id}/work-week",get(crate::member_work_week_handlers::MemberWorkWeekHandlers::get))
.route("/api/v1/me/permissions",get(crate::effective_permissions_handlers::EffectivePermissionsHandlers::get))
.route("/api/v1/team/members",get(crate::member_email_handlers::MemberEmailHandlers::team_members))
.route("/api/v1/settings/members",get(crate::member_email_handlers::MemberEmailHandlers::members))
.route("/api/v1/settings/members/{id}/position",axum::routing::patch(crate::member_position_handlers::MemberPositionHandlers::change))
.route("/api/v1/settings/members/{id}/role",get(crate::member_role_handlers::MemberRoleHandlers::get).put(crate::member_role_handlers::MemberRoleHandlers::change))
.route("/api/v1/settings/invitations",get(crate::member_email_handlers::MemberEmailHandlers::invitations).post(crate::member_email_handlers::MemberEmailHandlers::create))
.route("/api/v1/settings/invitations/{id}/resend",post(crate::member_email_handlers::MemberEmailHandlers::resend))
.route("/api/v1/settings/invitations/{id}/revoke",post(crate::member_email_handlers::MemberEmailHandlers::revoke))
.route("/api/v1/settings/email-contents",get(crate::email_content_handlers::EmailContentHandlers::list))
.route("/api/v1/settings/email-contents/{kind}/{language}",get(crate::email_content_handlers::EmailContentHandlers::get).put(crate::email_content_handlers::EmailContentHandlers::change))
.route("/api/v1/settings/email-template",get(crate::email_template_handlers::EmailTemplateHandlers::get).put(crate::email_template_handlers::EmailTemplateHandlers::change))
.route("/api/v1/settings/smtp/tests",post(crate::smtp_test_handlers::SmtpTestHandlers::queue))
.route("/api/v1/settings/smtp/tests/{id}",get(crate::smtp_test_handlers::SmtpTestHandlers::status))
.route("/api/v1/settings/organization",get(crate::organization_settings_handlers::OrganizationSettingsHandlers::organization).patch(crate::organization_settings_handlers::OrganizationSettingsHandlers::change_organization))
.route("/api/v1/settings/organization/logo",axum::routing::put(crate::organization_settings_handlers::OrganizationSettingsHandlers::change_logo).layer(DefaultBodyLimit::max(3*1024*1024)))
.route("/api/v1/settings/services/{kind}",get(crate::installation_service_handlers::InstallationServiceHandlers::service).put(crate::installation_service_handlers::InstallationServiceHandlers::change_service))
.route("/api/v1/settings/notification-recipients",get(crate::notification_recipient_handlers::NotificationRecipientHandlers::recipients))
.route("/api/v1/settings/notification-recipients/{actor}",axum::routing::put(crate::notification_recipient_handlers::NotificationRecipientHandlers::change_recipient))
.route("/api/v1/settings/general",get(crate::installation_preferences_handlers::InstallationPreferencesHandlers::get).put(crate::installation_preferences_handlers::InstallationPreferencesHandlers::change))
.route("/api/v1/events",get(crate::live_update_handlers::LiveUpdateHandlers::subscribe))
.route("/api/v1/task-types",get(crate::task_type_catalog_handlers::TaskTypeCatalogHandlers::list).post(crate::task_type_catalog_handlers::TaskTypeCatalogHandlers::create))
.route("/api/v1/task-types/{code}/change",post(crate::task_type_catalog_handlers::TaskTypeCatalogHandlers::change))
.route("/api/v1/projects/{project_id}/task-type-selection",get(crate::task_type_catalog_handlers::TaskTypeCatalogHandlers::selection).put(crate::task_type_catalog_handlers::TaskTypeCatalogHandlers::select))
            .route("/api/v1/git-authors",get(crate::git_author_handlers::GitAuthorHandlers::authors))
            .route("/api/v1/git-authors/{id}/{action}",post(crate::git_author_handlers::GitAuthorHandlers::archive))
            .route("/api/v1/git-author-aliases",get(crate::git_author_handlers::GitAuthorHandlers::aliases))
            .route("/api/v1/git-author-aliases/options",get(crate::git_author_handlers::GitAuthorHandlers::options))
            .route("/api/v1/members/{id}/git-aliases",get(crate::git_author_handlers::GitAuthorHandlers::member_aliases).post(crate::git_author_handlers::GitAuthorHandlers::create))
            .route("/api/v1/members/{id}/git-aliases/{alias_id}/revoke",post(crate::git_author_handlers::GitAuthorHandlers::revoke))
            .route("/api/v1/tasks/{task_id}/commits/{commit_id}/remove",post(crate::task_commit_handlers::TaskCommitHandlers::remove))
.route("/api/v1/tasks/{task_id}/commits",get(crate::task_commit_handlers::TaskCommitHandlers::list).post(crate::task_commit_handlers::TaskCommitHandlers::bind))
.route("/api/v1/projects/{project_id}/releases/{release_id}/composition",get(crate::task_release_handlers::TaskReleaseHandlers::compositions).post(crate::task_release_handlers::TaskReleaseHandlers::compose))
.route("/api/v1/projects/{project_id}/releases",get(crate::task_release_handlers::TaskReleaseHandlers::list).post(crate::task_release_handlers::TaskReleaseHandlers::create))
.route("/api/v1/projects/{project_id}/releases/{release_id}",get(crate::task_release_handlers::TaskReleaseHandlers::get))
.route("/api/v1/projects/{project_id}/releases/{release_id}/tasks",post(crate::task_release_handlers::TaskReleaseHandlers::add))
.route("/api/v1/tasks/{task_id}/complete-general",post(crate::general_task_completion_handlers::GeneralTaskCompletionHandlers::complete))
.route("/api/v1/products",get(crate::product_handlers::ProductHandlers::list).post(crate::product_handlers::ProductHandlers::save))
.route("/api/v1/requests/products",get(crate::product_handlers::ProductHandlers::public_list))
.route("/api/v1/knowledge-reviews",get(crate::knowledge_review_handlers::KnowledgeReviewHandlers::list).post(crate::knowledge_review_handlers::KnowledgeReviewHandlers::request))
.route("/api/v1/knowledge-reviews/{id}",get(crate::knowledge_review_handlers::KnowledgeReviewHandlers::get).post(crate::knowledge_review_handlers::KnowledgeReviewHandlers::decide))
.route("/api/v1/requests",get(crate::service_request_handlers::ServiceRequestHandlers::list).post(crate::service_request_handlers::ServiceRequestHandlers::create))
.route("/api/v1/requests/reports",get(crate::service_request_handlers::ServiceRequestHandlers::report))
.route("/api/v1/requests/types",get(crate::service_request_handlers::ServiceRequestHandlers::types).post(crate::service_request_handlers::ServiceRequestHandlers::save_type))
.route("/api/v1/requests/linked",get(crate::service_request_handlers::ServiceRequestHandlers::linked))
.route("/api/v1/requests/{id}",get(crate::service_request_handlers::ServiceRequestHandlers::get).post(crate::service_request_handlers::ServiceRequestHandlers::change))
.route("/api/v1/requests/{id}/comments",post(crate::service_request_handlers::ServiceRequestHandlers::comment))
.route("/api/v1/requests/{id}/links",post(crate::service_request_handlers::ServiceRequestHandlers::link))
.route("/api/v1/requests/{id}/attachments",post(crate::service_request_handlers::ServiceRequestHandlers::upload).layer(DefaultBodyLimit::max(21*1024*1024)))
.route("/api/v1/requests/{id}/attachments/{attachment}",get(crate::service_request_handlers::ServiceRequestHandlers::content))
.route("/api/v1/calendar/work-blocks",get(crate::personal_work_block_handlers::PersonalWorkBlockHandlers::list).post(crate::personal_work_block_handlers::PersonalWorkBlockHandlers::create))
.route("/api/v1/calendar/work-blocks/{id}",post(crate::personal_work_block_handlers::PersonalWorkBlockHandlers::change))
.route("/api/v1/calendar/work-blocks/{id}/remove",post(crate::personal_work_block_handlers::PersonalWorkBlockHandlers::remove))
.route("/api/v1/tasks/{task_id}/workflow-migrations",post(crate::task_workflow_migration_handlers::TaskWorkflowMigrationHandlers::migrate))
.route("/api/v1/tasks/{task_id}/work-logs",get(crate::task_work_log_handlers::TaskWorkLogHandlers::list).post(crate::task_work_log_handlers::TaskWorkLogHandlers::create))
.route("/api/v1/tasks/{task_id}/work-logs/{log_id}",post(crate::task_work_log_handlers::TaskWorkLogHandlers::change))
.route("/api/v1/tasks/{task_id}/work-logs/{log_id}/remove",post(crate::task_work_log_handlers::TaskWorkLogHandlers::remove))
            .route("/api/v1/tasks/{task_id}/relations",get(crate::task_relation_handlers::TaskRelationHandlers::get))
.route("/api/v1/tasks/{task_id}/subtasks",post(crate::task_relation_handlers::TaskRelationHandlers::subtask_create))
.route("/api/v1/tasks/{task_id}/subtasks/{child_id}",post(crate::task_relation_handlers::TaskRelationHandlers::subtask_link))
.route("/api/v1/tasks/{task_id}/subtasks/{child_id}/remove",post(crate::task_relation_handlers::TaskRelationHandlers::subtask_remove))
.route("/api/v1/tasks/{task_id}/dependencies",post(crate::task_relation_handlers::TaskRelationHandlers::dependency_create))
.route("/api/v1/tasks/{task_id}/dependencies/{dependency_id}/remove",post(crate::task_relation_handlers::TaskRelationHandlers::dependency_remove))
.route("/api/v1/tasks",get(crate::visible_task_handlers::VisibleTaskHandlers::list).post(crate::task_pool_handlers::TaskPoolHandlers::create))
            .route("/api/v1/tasks/{task_id}/documents",get(crate::task_document_handlers::TaskDocumentHandlers::task_page).post(crate::task_document_handlers::TaskDocumentHandlers::upload).layer(DefaultBodyLimit::max(29*1024*1024)))
            .route("/api/v1/tasks/{task_id}/timeline",get(crate::task_timeline_handlers::TaskTimelineHandlers::get))
            .route("/api/v1/git/github/webhook-health",get(crate::github_handlers::GithubHandlers::webhook_health).post(crate::github_handlers::GithubHandlers::check_webhook))
            .route("/api/v1/stickers",get(crate::sticker_handlers::StickerHandlers::list).post(crate::sticker_handlers::StickerHandlers::upload).layer(DefaultBodyLimit::max(6*1024*1024)))
            .route("/api/v1/stickers/{id}",axum::routing::delete(crate::sticker_handlers::StickerHandlers::archive))
            .route("/api/v1/stickers/{id}/content",get(crate::sticker_handlers::StickerHandlers::content))
            .route("/api/v1/knowledge/files",get(crate::task_document_handlers::TaskDocumentHandlers::library))
            .route("/api/v1/knowledge/files/{id}/content",get(crate::task_document_handlers::TaskDocumentHandlers::content))
            .route("/api/v1/knowledge/organization/documents",get(crate::organization_knowledge_handlers::OrganizationKnowledgeHandlers::page).post(crate::organization_knowledge_handlers::OrganizationKnowledgeHandlers::create))
            .route("/api/v1/knowledge/organization/documents/{id}",get(crate::organization_knowledge_handlers::OrganizationKnowledgeHandlers::get).patch(crate::organization_knowledge_handlers::OrganizationKnowledgeHandlers::change))
            .route("/api/v1/members/{actor_id}/rating-evidence",post(crate::rating_evidence_handlers::RatingEvidenceHandlers::upload).layer(DefaultBodyLimit::max(29*1024*1024)))
            .route("/api/v1/rating-evidence/{id}/content",get(crate::rating_evidence_handlers::RatingEvidenceHandlers::content))
            .route("/api/v1/knowledge/images",post(crate::knowledge_image_handlers::KnowledgeImageHandlers::upload).layer(DefaultBodyLimit::max(6*1024*1024)))
            .route("/api/v1/knowledge/images/{id}/content",get(crate::knowledge_image_handlers::KnowledgeImageHandlers::content))
            .route("/api/v1/task-evidence",post(crate::task_evidence_handlers::TaskEvidenceHandlers::upload).layer(DefaultBodyLimit::max(6*1024*1024)))
            .route("/api/v1/task-evidence/{id}/content",get(crate::task_evidence_handlers::TaskEvidenceHandlers::content))
            .route("/api/v1/tasks/options",get(crate::task_pool_handlers::TaskPoolHandlers::options))
            .route("/api/v1/tasks/{task_id}",get(crate::task_pool_handlers::TaskPoolHandlers::get).patch(crate::task_pool_handlers::TaskPoolHandlers::change))
            .route("/api/v1/tasks/{task_id}/project",post(crate::task_project_handlers::TaskProjectHandlers::change))
            .route("/api/v1/tasks/{task_id}/knowledge",post(crate::task_knowledge_handlers::TaskKnowledgeHandlers::link))
            .route("/api/v1/tasks/{task_id}/knowledge/options",get(crate::task_knowledge_handlers::TaskKnowledgeHandlers::options))
            .route("/api/v1/tasks/{task_id}/take",post(crate::task_pool_handlers::TaskPoolHandlers::take))
            .route("/api/v1/tasks/{task_id}/pause",post(crate::task_pool_handlers::TaskPoolHandlers::pause))
            .route("/api/v1/tasks/{task_id}/resume",post(crate::task_pool_handlers::TaskPoolHandlers::resume))
            .route("/api/v1/projects/{project_id}/tasks/{task_id}/take",post(crate::task_pool_handlers::TaskPoolHandlers::project_take))
            .route("/api/v1/tasks/{task_id}/transition-runs",get(crate::task_transition_run_handlers::TaskTransitionRunHandlers::list))
            .route("/api/v1/tasks/{task_id}/transition-runs/{run_id}/retry",post(crate::task_transition_run_handlers::TaskTransitionRunHandlers::retry))
            .route("/api/v1/tasks/{task_id}/transition-repositories/{repository_id}/branches/{*branch}",get(crate::task_transition_run_handlers::TaskTransitionRunHandlers::branch))
            .route("/api/v1/tasks/{task_id}/transitions/{transition_id}",post(crate::task_pool_handlers::TaskPoolHandlers::transition))
            .route("/api/v1/tasks/{task_id}/approvals",post(crate::task_pool_handlers::TaskPoolHandlers::approval_grant))
            .route("/api/v1/tasks/{task_id}/approvals/{approval_id}",get(crate::task_pool_handlers::TaskPoolHandlers::approval_get))
            .route("/api/v1/tasks/{task_id}/approvals/{approval_id}/revoke",post(crate::task_pool_handlers::TaskPoolHandlers::approval_revoke))
            .route("/api/v1/tasks/{task_id}/history",get(crate::task_pool_handlers::TaskPoolHandlers::history))
            .route("/api/v1/tasks/{task_id}/comments",get(crate::task_pool_handlers::TaskPoolHandlers::comments).post(crate::task_pool_handlers::TaskPoolHandlers::comment_add))
            .route("/api/v1/me/tasks",get(crate::visible_task_handlers::VisibleTaskHandlers::list))
            .route("/api/v1/me/task-types",get(crate::visible_task_handlers::VisibleTaskHandlers::types))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests/{number}/links",get(crate::pull_request_link_handlers::PullRequestLinkHandlers::list).post(crate::pull_request_link_handlers::PullRequestLinkHandlers::create))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests/{number}/links/{id}/remove",post(crate::pull_request_link_handlers::PullRequestLinkHandlers::remove))

            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests/{number}/reviews",get(crate::code_review_handlers::CodeReviewHandlers::list).post(crate::code_review_handlers::CodeReviewHandlers::request))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests/{number}/reviews/{id}",get(crate::code_review_handlers::CodeReviewHandlers::get))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests",get(crate::pull_request_handlers::PullRequestHandlers::list))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests/{number}",get(crate::pull_request_handlers::PullRequestHandlers::detail))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/pull-requests/{number}/{part}",get(crate::pull_request_handlers::PullRequestHandlers::part))

            .route("/api/v1/projects/{project_id}/evidence",post(crate::evidence_handlers::EvidenceHandlers::upload).layer(DefaultBodyLimit::max(6*1024*1024)))
            .route("/api/v1/projects/{project_id}/evidence/{id}",get(crate::evidence_handlers::EvidenceHandlers::get))
            .route("/api/v1/projects/{project_id}/evidence/{id}/content",get(crate::evidence_handlers::EvidenceHandlers::content))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/evidence",get(crate::evidence_handlers::EvidenceHandlers::list))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/analyses",post(crate::reasoning_handlers::ReasoningHandlers::evaluate))
            .route("/api/v1/projects/{project_id}/repositories/{repository_id}/code/query", post(crate::repository_code_handlers::RepositoryCodeHandlers::query))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/knowledge",get(crate::knowledge_handlers::KnowledgeHandlers::list).post(crate::knowledge_handlers::KnowledgeHandlers::contribute))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/knowledge/{knowledge_id}",get(crate::knowledge_handlers::KnowledgeHandlers::get).patch(crate::knowledge_handlers::KnowledgeHandlers::revise))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/knowledge/{knowledge_id}/review",post(crate::knowledge_handlers::KnowledgeHandlers::review))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/pending",get(crate::knowledge_handlers::KnowledgeHandlers::pending))
.route("/api/v1/tasks/{id}/stages",get(crate::task_transition_run_handlers::TaskTransitionRunHandlers::stages))
.route("/api/v1/projects/{project_id}/tasks/{task_id}/move",post(crate::task_lifecycle_handlers::TaskLifecycleHandlers::movement))
.route("/api/v1/projects/{project_id}/tasks/{task_id}/workflow-migrations",post(crate::task_lifecycle_handlers::TaskLifecycleHandlers::migration))
.route("/api/v1/projects/{project_id}/documents/{document_id}/versions/{version_id}/approvals/{approval_id}",get(crate::document_approval_handlers::DocumentApprovalHandlers::get))
.route("/api/v1/projects/{project_id}/documents/{document_id}/versions/{version_id}/approvals/{approval_id}/revoke",post(crate::document_approval_handlers::DocumentApprovalHandlers::revoke))
.route("/api/v1/projects/{project_id}/task-types/{type_code}/change",post(crate::work_catalog_handlers::WorkCatalogHandlers::task_type_change))
.route("/api/v1/projects/{project_id}/approval-requests",get(crate::approval_request_handlers::ApprovalRequestHandlers::page).post(crate::approval_request_handlers::ApprovalRequestHandlers::create))
.route("/api/v1/projects/{project_id}/approval-requests/{approval_request_id}",get(crate::approval_request_handlers::ApprovalRequestHandlers::get))
.route("/api/v1/projects/{project_id}/approval-requests/{approval_request_id}/decisions",post(crate::approval_request_handlers::ApprovalRequestHandlers::decide))
.route("/api/v1/projects/{project_id}/approval-requests/{approval_request_id}/cancel",post(crate::approval_request_handlers::ApprovalRequestHandlers::cancel))
.route("/api/v1/projects/{project_id}/documents/{document_id}/links",get(crate::traceability_handlers::TraceabilityHandlers::document_links_list).post(crate::traceability_handlers::TraceabilityHandlers::document_link_create))
.route("/api/v1/projects/{project_id}/documents/{document_id}/links/{link_id}/remove",post(crate::traceability_handlers::TraceabilityHandlers::document_link_remove))
.route("/api/v1/projects/{project_id}/tasks/{task_id}/documents",get(crate::traceability_handlers::TraceabilityHandlers::task_documents_list))
.route("/api/v1/projects/{project_id}/context-packages",post(crate::traceability_handlers::TraceabilityHandlers::context_package_create))
.route("/api/v1/projects/{project_id}/context-packages/{package_id}",get(crate::traceability_handlers::TraceabilityHandlers::context_package_get))
.route("/api/v1/projects/{project_id}/context-packages/{package_id}/export",get(crate::traceability_handlers::TraceabilityHandlers::context_package_export))
.route("/api/v1/projects/{project_id}/context-manifests",post(crate::project_context_handlers::ProjectContextHandlers::create))
.route("/api/v1/projects/{project_id}/context-manifests/{manifest_id}",get(crate::project_context_handlers::ProjectContextHandlers::get))
.route("/api/v1/projects/{project_id}/context-manifests/{manifest_id}/status",get(crate::project_context_handlers::ProjectContextHandlers::status))

.route("/api/v1/projects/{project_id}/documents/{document_id}/history",get(crate::document_catalog_handlers::DocumentCatalogHandlers::history))
.route("/api/v1/projects/{project_id}/documents",post(crate::document_handlers::DocumentHandlers::create).get(crate::document_catalog_handlers::DocumentCatalogHandlers::list))
.route("/api/v1/projects/{project_id}/documents/{document_id}",get(crate::document_handlers::DocumentHandlers::get).patch(crate::document_handlers::DocumentHandlers::change))
.route("/api/v1/projects/{project_id}/documents/{document_id}/versions",get(crate::document_handlers::DocumentHandlers::versions_list).post(crate::document_handlers::DocumentHandlers::revise))
.route("/api/v1/projects/{project_id}/documents/{document_id}/versions/{version_id}",get(crate::document_handlers::DocumentHandlers::version_get))
.route("/api/v1/projects/{project_id}/documents/{document_id}/versions/{version_id}/transitions/{transition_id}",post(crate::document_handlers::DocumentHandlers::transition))
.route("/api/v1/projects/{project_id}/documents/{document_id}/publications",post(crate::document_handlers::DocumentHandlers::publish).get(crate::document_catalog_handlers::DocumentCatalogHandlers::publications))
.route("/api/v1/projects/{project_id}/documents/{document_id}/versions/{version_id}/export",get(crate::document_handlers::DocumentHandlers::export))
.route("/api/v1/projects/{project_id}/task-templates",get(crate::task_template_handlers::TaskTemplateHandlers::list).post(crate::task_template_handlers::TaskTemplateHandlers::create))
.route("/api/v1/projects/{project_id}/task-templates/{template_id}",get(crate::task_template_handlers::TaskTemplateHandlers::get).patch(crate::task_template_handlers::TaskTemplateHandlers::change))
.route("/api/v1/projects/{project_id}/task-templates/{template_id}/versions",get(crate::task_template_handlers::TaskTemplateHandlers::versions).post(crate::task_template_handlers::TaskTemplateHandlers::publish))
.route("/api/v1/projects/{project_id}/task-templates/{template_id}/versions/{version_id}",get(crate::task_template_handlers::TaskTemplateHandlers::version))
.route("/api/v1/projects/{project_id}/task-templates/apply",post(crate::task_template_handlers::TaskTemplateHandlers::apply))

.route("/api/v1/projects/{project_id}/task-dependencies",post(crate::task_collaboration_handlers::TaskCollaborationHandlers::dependency_create))
.route("/api/v1/projects/{project_id}/task-dependencies/{dependency_id}/resolve",post(crate::task_collaboration_handlers::TaskCollaborationHandlers::dependency_resolve))
.route("/api/v1/projects/{project_id}/task-dependencies/{dependency_id}/remove",post(crate::task_collaboration_handlers::TaskCollaborationHandlers::dependency_remove))
.route("/api/v1/projects/{project_id}/tasks/{task_id}/dependencies",get(crate::task_collaboration_handlers::TaskCollaborationHandlers::dependencies))
.route("/api/v1/projects/{project_id}/tasks/{task_id}/comments",get(crate::task_collaboration_handlers::TaskCollaborationHandlers::comments).post(crate::task_collaboration_handlers::TaskCollaborationHandlers::comment_add))

            .route(
                "/api/v1/projects/{project_id}/tasks/{task_id}/approvals/{approval_id}",
                get(crate::task_approval_handlers::TaskApprovalHandlers::get),
            )
            .route(
                "/api/v1/projects/{project_id}/tasks/{task_id}/approvals/{approval_id}/revoke",
                post(crate::task_approval_handlers::TaskApprovalHandlers::revoke),
            )
            .route("/api/v1/knowledge/contributions",get(crate::knowledge_contribution_handlers::KnowledgeContributionHandlers::page).post(crate::knowledge_contribution_handlers::KnowledgeContributionHandlers::create).layer(DefaultBodyLimit::max(7*1024*1024)))
            .route("/api/v1/knowledge/contributions/{id}",get(crate::knowledge_contribution_handlers::KnowledgeContributionHandlers::get).patch(crate::knowledge_contribution_handlers::KnowledgeContributionHandlers::change).layer(DefaultBodyLimit::max(7*1024*1024)))
            .route("/api/v1/knowledge/contributions/{id}/content",get(crate::knowledge_contribution_handlers::KnowledgeContributionHandlers::file))
            .route(
                "/api/v1/projects/{project_id}/task-types",
                get(crate::work_catalog_handlers::WorkCatalogHandlers::task_types_list).post(crate::work_item_handlers::WorkItemHandlers::type_create),
            )
            .route(
                "/api/v1/projects/{project_id}/backlogs",
                get(crate::work_catalog_handlers::WorkCatalogHandlers::backlogs_list).post(crate::work_item_handlers::WorkItemHandlers::backlog_create),
            )
            .route(
                "/api/v1/projects/{project_id}/backlogs/{backlog_id}",
                get(crate::work_item_handlers::WorkItemHandlers::backlog_get)
                    .patch(crate::work_item_handlers::WorkItemHandlers::backlog_change),
            )
            .route(
                "/api/v1/projects/{project_id}/backlogs/{backlog_id}/order",
                post(crate::work_item_handlers::WorkItemHandlers::backlog_reorder),
            )
            .route(
                "/api/v1/projects/{project_id}/tasks",
                get(crate::work_item_handlers::WorkItemHandlers::task_list)
                    .post(crate::work_item_handlers::WorkItemHandlers::task_create),
            )
            .route(
                "/api/v1/projects/{project_id}/tasks/{task_id}",
                get(crate::work_item_handlers::WorkItemHandlers::task_get)
                    .patch(crate::work_item_handlers::WorkItemHandlers::task_change),
            )
            .route(
                "/api/v1/projects/{project_id}/tasks/{task_id}/transitions/{transition_id}",
                post(crate::work_item_handlers::WorkItemHandlers::task_transition),
            )
            .route(
                "/api/v1/projects/{project_id}/tasks/{task_id}/history",
                get(crate::work_item_handlers::WorkItemHandlers::task_history),
            )
            .route(
                "/api/v1/projects/{project_id}/tasks/{task_id}/approvals",
                post(crate::work_item_handlers::WorkItemHandlers::approval_grant),
            )
            .route("/api/v1/developers",get(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::directory))
            .route("/api/v1/me/workflow-tasks",get(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::board))
            .route("/api/v1/workflows", get(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::list).post(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::create))
            .route("/api/v1/workflows/{workflow_id}", get(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::get).patch(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::change))
            .route("/api/v1/workflows/{workflow_id}/publish", post(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::publish))
            .route("/api/v1/workflows/{workflow_id}/versions", get(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::versions))
            .route("/api/v1/workflows/{workflow_id}/versions/{version_id}", get(crate::developer_workflow_handlers::DeveloperWorkflowHandlers::version))
            .route("/api/v1/workflows/{workflow_id}/developers", get(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::members))
            .route("/api/v1/actors/{actor_id}/workflows", get(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::list))
            .route("/api/v1/actors/{actor_id}/workflow", get(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::get).put(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::assign).delete(crate::developer_workflow_member_handlers::DeveloperWorkflowMemberHandlers::remove))
            .route(
                "/api/v1/projects/{project_id}/workflows",
                get(crate::workflow_handlers::WorkflowHandlers::list)
                    .post(crate::workflow_handlers::WorkflowHandlers::create),
            )
            .route(
                "/api/v1/projects/{project_id}/workflows/{workflow_id}",
                get(crate::workflow_handlers::WorkflowHandlers::get)
                    .patch(crate::workflow_handlers::WorkflowHandlers::change),
            )
            .route(
                "/api/v1/projects/{project_id}/workflows/{workflow_id}/publish",
                post(crate::workflow_handlers::WorkflowHandlers::publish),
            )
            .route(
                "/api/v1/projects/{project_id}/workflows/{workflow_id}/versions",
                get(crate::workflow_handlers::WorkflowHandlers::versions),
            )
            .route(
                "/api/v1/projects/{project_id}/workflows/{workflow_id}/versions/{version_id}",
                get(crate::workflow_handlers::WorkflowHandlers::version),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories/{repository_id}/sync",
                post(RepositoryJobHandlers::sync),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories/{repository_id}/index",
                post(RepositoryJobHandlers::index),
            )
            .route(
                "/api/v1/projects/{project_id}/jobs/{job_id}",
                get(RepositoryJobHandlers::get),
            )
            .route(
                "/api/v1/projects/{project_id}/jobs/{job_id}/cancel",
                post(RepositoryJobHandlers::cancel),
            )
            .route(
                "/api/v1/projects/{project_id}/git/connections/{connection_id}/repositories",
                get(RemoteRepositoryHandlers::list),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories/{repository_id}/webhooks/gitlab",
                get(GitWebhookHandlers::list).post(GitWebhookHandlers::register),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories/{repository_id}/webhooks/gitlab/{id}",
                delete(GitWebhookHandlers::revoke),
            )
            .route(
                "/api/v1/git/github/manifest/start",
                post(GithubManifestHandlers::start),
            )
            .route(
                "/api/v1/git/github/configuration",
                get(GithubHandlers::configuration)
                    .post(GithubHandlers::configure)
                    .patch(GithubHandlers::change_state),
            )
            .route("/api/v1/git/github/start", post(GithubHandlers::start))
            .route(
                "/api/v1/git/github/authorizations/{id}/installations",
                get(GithubHandlers::installations),
            )
            .route(
                "/api/v1/git/github/authorizations/{id}/connections",
                post(GithubHandlers::select),
            )
            .route("/api/v1/git/connections", get(GitHandlers::list))
            .route("/api/v1/git/connections/{connection_id}/repositories", get(RemoteRepositoryHandlers::available))
            .route(
                "/api/v1/projects/{project_id}/git/connections",
                get(GitHandlers::project_connections),
            )
            .route(
                "/api/v1/git/gitlab/configuration",
                get(GitHandlers::configuration)
                    .post(GitHandlers::configure)
                    .patch(GitHandlers::change_state),
            )
            .route("/api/v1/git/gitlab/start", post(GitHandlers::start))
            .route(
                "/api/v1/git/connections/{connection_id}",
                delete(GitHandlers::revoke),
            )
            .route(
                "/api/v1/git/connections/{connection_id}/projects/{project_id}",
                post(GitHandlers::share).delete(GitHandlers::unshare),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories/{repository_id}/authorize",
                post(GitHandlers::authorize),
            )
            .route(
                "/api/v1/auth/oidc/providers",
                get(OidcHandlers::providers).post(OidcHandlers::configure),
            )
            .route("/api/v1/auth/oidc/providers/{id}", axum::routing::patch(OidcHandlers::change))
            .route("/api/v1/users/oidc", post(OidcHandlers::create_user))
            .route(
                "/api/v1/actors/{id}/identities/oidc",
                post(OidcHandlers::assign),
            )
            .route(
                "/api/v1/auth/methods",
                axum::routing::put(OidcHandlers::change_methods),
            )
            .route(
                "/mcp",
                post(PlatformMcpTransport::post).get(PlatformMcpTransport::get).layer(DefaultBodyLimit::max(29*1024*1024)),
            )
            .route("/api/v1/me/email",get(crate::user_email_handlers::UserEmailHandlers::get).post(crate::user_email_handlers::UserEmailHandlers::request).put(crate::user_email_handlers::UserEmailHandlers::verify))
            .route("/api/v1/settings/members/{id}/lifecycle",axum::routing::patch(crate::member_lifecycle_handlers::MemberLifecycleHandlers::change))
            .route("/api/v1/auth/logout", post(IdentityHandlers::logout))
            .route("/api/v1/members/{id}/totp",get(crate::identity_gate_handlers::IdentityGateHandlers::status).patch(crate::identity_gate_handlers::IdentityGateHandlers::exempt))
.route("/api/v1/me/passkeys",get(crate::passkey_handlers::PasskeyHandlers::status).post(crate::passkey_handlers::PasskeyHandlers::start))
.route("/api/v1/me/security-method",axum::routing::patch(crate::passkey_handlers::PasskeyHandlers::set_method))
.route("/api/v1/me/passkeys/register/{ceremony_id}",post(crate::passkey_handlers::PasskeyHandlers::finish))
            .route("/api/v1/users", post(IdentityHandlers::create_user))
            .route("/api/v1/actors", post(IdentityHandlers::create_actor))
            .route(
                "/api/v1/actors/{id}/suspend",
                post(IdentityHandlers::suspend_actor),
            )
            .route(
                "/api/v1/actors/{id}/grants",
                post(GrantHandlers::grant).delete(GrantHandlers::revoke),
            )
            .route(
                "/api/v1/actors/{id}/api-keys",
                get(KeyHandlers::list).post(KeyHandlers::issue),
            )
            .route("/api/v1/me/mcp-keys", get(crate::personal_mcp_key_handlers::PersonalMcpKeyHandlers::list).post(crate::personal_mcp_key_handlers::PersonalMcpKeyHandlers::issue))
            .route("/api/v1/me/mcp-keys/{id}", delete(crate::personal_mcp_key_handlers::PersonalMcpKeyHandlers::revoke))
            .route("/api/v1/api-keys/{id}", delete(KeyHandlers::revoke))
            .route("/api/v1/api-keys/{id}/rotate", post(KeyHandlers::rotate))
            .route(
                "/api/v1/projects",
                get(ProjectHandlers::list).post(ProjectHandlers::create),
            )
            .route(
                "/api/v1/projects/{id}",
                get(ProjectHandlers::get).put(ProjectHandlers::change),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories",
                get(RepositoryHandlers::list).post(RepositoryHandlers::register),
            )
            .route(
                "/api/v1/projects/{project_id}/repositories/{repository_id}",
                get(RepositoryHandlers::get).put(RepositoryHandlers::change),
            )
.route("/api/v1/settings/navigation",get(crate::navigation_order_handlers::NavigationOrderHandlers::get).put(crate::navigation_order_handlers::NavigationOrderHandlers::change))
.route("/api/v1/analysis-profiles",get(crate::analysis_profile_handlers::AnalysisProfileHandlers::list).post(crate::analysis_profile_handlers::AnalysisProfileHandlers::import).layer(DefaultBodyLimit::max(nexora_language::AnalysisProfilePackage::MAX_BYTES)))
.route("/api/v1/analysis-profiles/{id}",get(crate::analysis_profile_handlers::AnalysisProfileHandlers::get))
.route("/api/v1/analysis-profiles/{id}/export",get(crate::analysis_profile_handlers::AnalysisProfileHandlers::export))
.route("/api/v1/projects/{project_id}/repositories/{repository_id}/analysis-profile",get(crate::analysis_profile_handlers::AnalysisProfileHandlers::assignment).put(crate::analysis_profile_handlers::AnalysisProfileHandlers::change))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                ApiMiddleware::authenticate,
            ));
        Router::new()
            .route(
                "/api/v1/git/github/webhook",
                post(GitWebhookHandlers::github),
            )
            .route(
                "/api/v1/git/gitlab/webhooks/{id}",
                post(GitWebhookHandlers::gitlab),
            )
            .route(
                "/api/v1/git/github/manifest/callback",
                get(GithubManifestHandlers::callback),
            )
            .route("/api/v1/git/gitlab/callback", get(GitHandlers::callback))
            .route("/api/v1/git/github/callback", get(GithubHandlers::callback))
            .route(
                "/api/v1/auth/oidc/providers/{provider_id}/start",
                post(OidcHandlers::start),
            )
            .route("/api/v1/auth/oidc/callback", get(OidcHandlers::callback))
            .route("/api/v1/auth/methods", get(OidcHandlers::methods))
.route("/api/v1/public/organization",get(crate::organization_settings_handlers::OrganizationSettingsHandlers::branding))
.route("/api/v1/public/organization/logo",get(crate::organization_settings_handlers::OrganizationSettingsHandlers::logo))
.route("/api/v1/public/preferences",get(crate::installation_preferences_handlers::InstallationPreferencesHandlers::public_get))
.route("/api/v1/public/preferences/events",get(crate::installation_preferences_events::InstallationPreferencesEvents::subscribe))
            .route(
                "/openapi.json",
                get(|| async {
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        include_str!("../../../docs/platform/openapi.json"),
                    )
                }),
            )
            .route("/ready", get(Self::ready))
            .route(
                "/health",
                get(|| async { Json(json!({"service":"nexora-api","status":"up"})) }),
            )
            .route("/api/v1/auth/session",get(crate::browser_session_handlers::BrowserSessionHandlers::get).post(crate::browser_session_handlers::BrowserSessionHandlers::adopt).delete(crate::browser_session_handlers::BrowserSessionHandlers::delete))
            .route("/api/v1/auth/login", post(IdentityHandlers::login))
            .route("/api/v1/auth/passkey", post(crate::passkey_handlers::PasskeyHandlers::authenticate_start))
            .route("/api/v1/auth/passkey/{ceremony_id}", post(crate::passkey_handlers::PasskeyHandlers::authenticate_finish))
            .route("/api/v1/auth/gate",post(crate::identity_gate_handlers::IdentityGateHandlers::advance))
.route("/api/v1/auth/invitations/inspect",post(crate::public_email_access_handlers::PublicEmailAccessHandlers::inspect))
.route("/api/v1/auth/invitations/accept",post(crate::public_email_access_handlers::PublicEmailAccessHandlers::accept))
.route("/api/v1/auth/password/request",post(crate::public_email_access_handlers::PublicEmailAccessHandlers::request_recovery))
.route("/api/v1/auth/password/complete",post(crate::public_email_access_handlers::PublicEmailAccessHandlers::complete_recovery))
            .merge(secured)
            .fallback(|| async { ApiError(PlatformError::NotFound) })
            .layer(DefaultBodyLimit::max(1_048_576))
            .layer(middleware::from_fn(ApiMiddleware::request_id))
            .with_state(state)
    }
    async fn ready(State(state): State<ApiState>) -> (StatusCode, Json<serde_json::Value>) {
        let ready = state.run(|store| store.ready()).await.unwrap_or(false);
        (
            if ready {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            },
            Json(json!({"ready":ready})),
        )
    }
}
