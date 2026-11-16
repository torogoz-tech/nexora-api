//! Concrete PostgreSQL-backed application services for the Nexora platform.
mod backlog_order_store;
mod backlog_store;
mod local_snapshot_adoption_store;
mod local_snapshot_request;
pub use local_snapshot_request::*;
mod local_snapshot_publication_store;
mod local_snapshot_scope;
mod local_snapshot_verifier;
mod local_source_lifecycle_store;
mod local_source_operator_authority;
mod platform_authority;
mod repository_access;
mod repository_binding_store;
mod repository_code_store;
mod repository_job_lease;
mod repository_job_source;
mod repository_knowledge_store;
mod repository_snapshot_scope;
mod task_definition_store;
mod task_history_writer;
mod task_policy_store;
mod task_query_store;
mod task_transition_store;
mod task_type_store;
mod workflow_definition_store;
mod workflow_publication_store;
mod workflow_query_store;
pub use repository_job_lease::*;
mod git_commit_identity;
mod repository_job_execution_authority;
mod repository_job_lease_store;
mod repository_job_publication_store;
mod repository_job_source_plan;
pub use repository_job_source_plan::*;
mod repository_job_source_plan_store;
mod repository_job_source_request;
mod repository_job_store;
mod repository_runtime_scope;
pub use repository_runtime_scope::*;
mod secret_generator;
pub use secret_generator::*;
mod platform_store;
pub use platform_store::*;
mod audit_writer;
mod authentication_store;
mod credential_store;
mod grant_store;
mod identity_store;
mod passkey_store;
mod password_credential;
mod project_store;

mod master_key;
mod mutation_request;
mod receipt_writer;

mod platform_migrations;
mod repository_store;

#[cfg(test)]
mod github_rate_limit_tests;
#[cfg(test)]
mod provider_cipher_tests;

mod provider_secret_cipher;
pub use provider_secret_cipher::*;

mod provider_secret_context;
pub use provider_secret_context::*;

mod provider_secret_envelope;
pub use provider_secret_envelope::*;

mod secret_authority;
pub use secret_authority::*;
mod oidc_network_policy;
pub use oidc_network_policy::*;

mod oidc_http_client;

mod oidc_client;

mod oidc_pending_secrets;

mod oidc_settings_store;

mod oidc_account_store;

mod oidc_authorization_store;

mod git_network_policy;

mod gitlab_tokens;

mod gitlab_client;

mod gitlab_settings_store;

mod git_connection_store;

mod gitlab_authorization_store;

mod gitlab_token_store;

mod git_repository_store;

pub use git_network_policy::*;

mod git_initiator;

mod github_secrets;

mod github_tokens;

mod github_jwt;

mod github_client;

mod github_settings_store;

mod github_authorization_store;

mod github_consent_store;

mod github_token_store;

mod github_repository_proof;
mod github_repository_token;

mod github_manifest_store;

mod webhook_proof;

mod webhook_signature;

mod webhook_delivery_writer;

mod source_event_writer;

mod github_webhook_store;

mod gitlab_webhook_store;

pub use webhook_proof::*;
mod git_configuration_state_store;
mod remote_repository_decoder;
mod remote_repository_store;
mod repository_job_failure_store;

mod task_approval_fingerprint;
mod task_approval_store;
mod task_transition_evaluation;

mod task_approval_query_store;
mod task_approval_revocation_store;

mod task_comment_query_store;
mod task_comment_store;
mod task_dependency_creation_store;
mod task_dependency_lifecycle_store;
mod task_dependency_lock;
mod task_dependency_policy;
mod task_dependency_query_store;

mod task_template_application_store;
mod task_template_checksum;
mod task_template_definition_store;
mod task_template_policy_store;
mod task_template_publication_store;
mod task_template_query_store;
mod task_template_validation;

mod document_content_validation;

mod document_version_checksum;

mod document_policy_store;

mod document_history_writer;

mod document_revision_store;

mod project_document_definition_store;

mod document_transition_store;

mod document_publication_store;

mod document_query_store;

mod document_export_renderer;

mod document_page_validation;

mod document_catalog_store;

mod document_history_query_store;

mod document_publication_query_store;

mod project_context_checksum;

mod project_context_policy;

mod project_context_manifest_store;

mod document_verified_context;

mod document_context_store;

mod document_link_scope;

mod document_link_policy;

mod document_link_history;

mod document_link_store;

mod document_link_query_store;

mod task_document_condition;

mod context_package_policy;

mod context_package_capture;

mod context_package_store;

mod context_package_renderer;

mod approval_operation_policy;
mod approval_operation_snapshot;
mod approval_request_authority;
mod approval_request_decision_store;
mod approval_request_definition_store;
mod approval_request_history_writer;
mod approval_request_query_store;
mod document_approval_fingerprint;
mod document_approval_store;
mod document_transition_evaluation;

mod backlog_query_store;
mod document_approval_query_store;
mod document_approval_revocation_store;
mod task_type_change_store;
mod task_type_query_store;

mod task_movement_store;
mod task_workflow_migration_store;

mod evidence_store;
mod evidence_validation;
#[cfg(test)]
mod evidence_validation_tests;
mod knowledge_receipt;
mod knowledge_request_identity;

mod development_policy_store;
mod task_work_store;

mod task_work_report_store;

mod repository_webhook_analysis_store;
mod repository_webhook_job_authority;

mod pull_request_decoder;
pub use pull_request_decoder::PullRequestDecoder;
mod pull_request_client;
mod pull_request_source;
mod pull_request_store;
pub use pull_request_store::PullRequestStore;

mod pull_request_links_store;
pub use pull_request_links_store::PullRequestLinksStore;

mod developer_workflow_api;
mod developer_workflow_store;

mod developer_task_policy;

mod developer_board_store;

mod workflow_read_authority;
mod workflow_read_scope;

mod task_read_authority;
mod task_read_predicate;
mod task_read_scope;
mod visible_task_api;
mod visible_task_store;

mod project_task_type_selection_store;
mod task_type_catalog_api;
mod task_type_catalog_store;

mod live_updates_store;

mod installation_preferences_store;

mod installation_service_store;
mod organization_profile_store;
mod service_runtime_settings;
pub use service_runtime_settings::ServiceRuntimeSettings;
mod email_notification;
pub use email_notification::EmailNotification;
mod notification_settings_store;

mod email_access_store;
mod email_action_delivery_store;
mod email_action_parameters;
mod email_content_store;
mod email_template_store;
mod email_token_cipher;
mod email_token_envelope;
mod installation_service_policy;
mod member_invitation_store;
mod password_recovery_store;

#[cfg(test)]
mod github_expanded_permissions_tests;
#[cfg(test)]
mod github_permissions_tests;

mod member_role_store;

mod feature_governance_policy;
mod feature_governance_store;

mod member_profile_api;
mod member_profile_store;
mod member_public_projection;
mod member_work_week_store;

mod member_avatar_authority;
mod member_avatar_store;

mod project_profile_api;
mod project_profile_store;

mod repository_profile_api;
mod repository_profile_metadata;
mod repository_profile_store;

mod developer_push_writer;
mod developer_rating_store;

mod labor_calendar_policy;
mod labor_calendar_request_store;
mod labor_calendar_store;

mod installation_timezone_store;
mod overtime_store;
mod working_hours_policy;

mod overtime_evidence_api_store;
mod overtime_evidence_store;

mod member_position_store;

mod effective_permissions_store;

mod task_scope_authority;

mod task_pause_store;
mod task_pool_store;
mod task_pool_workflow;

mod task_pool_api;

mod task_pool_options;

mod task_relation_lock;

mod task_relation_policy;

mod task_relation_query_store;

mod task_subtask_store;

mod task_relation_dependency_store;

mod task_relation_api;

mod git_author_identity;

mod git_author_policy;

mod git_author_observation_writer;

mod git_author_alias_store;

mod git_author_query_store;

mod git_author_api;

mod analysis_profile_api;
mod analysis_profile_store;

mod analysis_profile_report_store;
mod analysis_profile_selection_store;

mod navigation_order_api;
mod navigation_order_store;

mod personal_mcp_key_api;
mod personal_mcp_key_policy;
mod personal_mcp_key_store;

mod code_review_store;
mod prepared_code_review;
pub use code_review_store::CodeReviewStore;
pub use prepared_code_review::PreparedCodeReview;

mod calendar_activity_input;
mod calendar_activity_store;

mod task_presentation_store;

mod task_evidence_binding;
mod task_evidence_store;

mod task_knowledge_store;

mod task_project_store;

mod identity_gate_store;
mod login_gate;
mod totp_code;

mod oidc_verified_identity;

mod installation_oidc_policy;

mod local_login_availability;

mod browser_session_store;

mod email_verification_code;
mod user_email_store;

mod member_lifecycle_store;

mod workflow_color;

mod calendar_attendance_store;

mod overtime_approval_authority;

mod rating_feedback_store;

mod repository_file_store;

mod task_document_api;
mod task_document_store;

mod organization_knowledge_store;

mod organization_knowledge_authority;

mod rating_evidence_store;

mod organization_sticker_store;

mod task_workflow_timeline;

mod repository_commit_import_store;

mod github_webhook_health_store;

mod github_historical_commit_writer;
mod repository_branch_import_store;

mod repository_commit_store;

mod developer_workflow_authority;

mod git_author_archive_store;

mod rating_general_authority;

mod rating_feedback_authority;

mod team_member_store;

mod in_app_notification_change;
pub use in_app_notification_change::InAppNotificationChange;
mod in_app_notification_query;
pub use in_app_notification_query::InAppNotificationQuery;
mod in_app_notification_store;

mod daily_developer_analysis_policy;
mod daily_developer_analysis_store;

mod in_app_notification_title;

mod in_app_notification_body;

mod knowledge_contribution_api;
mod knowledge_contribution_policy;
mod knowledge_contribution_store;
mod knowledge_file;
mod knowledge_rag_store;
mod knowledge_synthesis_store;

mod repository_analysis_issues;

mod task_provider_operations;
mod task_transition_engine;
mod task_transition_provider;
mod task_transition_run_store;

mod task_transition_continuation_store;

mod task_transition_provider_authority;

mod labor_leave_reason;

mod labor_request_approval_authority;

mod knowledge_context_search;

mod member_weekly_rating;

mod task_acceptance_criteria;
mod task_purpose_policy;

mod feature_completion_store;
mod task_image_upload;

mod feature_proposal_read_store;

mod knowledge_image_store;

mod personal_work_block_store;
mod task_pool_migration_store;
mod task_work_log_api;
mod task_work_log_store;

mod task_stage_store;

mod task_pre_pr_review;

mod general_task_completion_store;

mod task_release_api;
mod task_release_store;

mod task_branch_origin;

mod task_delivery_evidence;

mod task_stage_query;

mod task_release_publication;

mod task_provider_merge;

mod task_release_composition;

#[cfg(test)]
extern crate self as nexora_platform_store;

mod task_review_knowledge;

mod task_subtask_integration;

mod task_subtask_stage_policy;

mod task_tested_revision;

mod task_code_start;
mod task_commit_store;

mod release_composition_store;
mod release_composition_worker;
mod task_contributor_code;
mod task_contributor_rating;

mod task_branch_selectors;

mod task_commit_binding_writer;

mod task_handoff_provider_evidence;

mod task_active_stage_predicate;

mod repository_branch_protection_provider;

mod branch_protection_preview;
mod repository_branch_protection_store;
pub use branch_protection_preview::*;
mod branch_protection_apply;
pub use branch_protection_apply::*;

mod request_workspace_store;
mod service_request_attachments;
mod service_request_links;
mod service_request_mutations;
mod service_request_policy;
mod service_request_store;

mod service_request_upload_digest;
pub use service_request_upload_digest::ServiceRequestUploadDigest;

mod knowledge_review_policy;
mod knowledge_review_store;
mod product_store;

mod service_request_reports;
