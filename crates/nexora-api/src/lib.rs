//! Authenticated HTTP and MCP platform boundary for Nexora.
mod local_operator_arguments;
pub use local_operator_arguments::*;
mod local_snapshot_arguments;
pub use local_snapshot_arguments::*;
mod local_source_revocation_arguments;
pub use local_source_revocation_arguments::*;
mod local_operator_secret;
mod local_operator_session;
mod local_snapshot_commands;
pub use local_snapshot_commands::*;
mod code_mcp_catalog;
mod code_mcp_query;
mod code_mcp_tools;
mod code_query_operation;
mod knowledge_rag_query;
mod mcp_workflow_change;
mod mcp_workflow_create;
mod mcp_workflow_list;
mod mcp_workflow_publish;
mod mcp_workflow_query;
mod mcp_workflow_version_query;
mod mcp_workflow_versions_query;
mod rabbitmq_job_wakeup_publisher;
mod repository_code_handlers;
mod repository_code_request;
#[cfg(test)]
mod repository_code_response_tests;
mod workflow_handlers;
mod workflow_list_query;
mod workflow_mcp_catalog;
mod workflow_mcp_tools;
mod workflow_versions_query;
pub use rabbitmq_job_wakeup_publisher::*;
mod rabbitmq_job_wakeup_dispatcher;
pub use rabbitmq_job_wakeup_dispatcher::*;
mod api_state;
mod repository_job_handlers;
pub use api_state::*;
mod api_server;
pub use api_server::*;
mod api_arguments;
mod api_error;
mod api_middleware;
mod create_actor_request;
mod create_project_request;
mod create_user_request;
mod grant_handlers;
mod grant_request;
mod identity_handlers;
mod issue_key_request;
mod key_handlers;
mod login_request;
mod passkey_handlers;
mod project_handlers;
mod rotate_key_request;
mod security_method_request;
pub use api_arguments::*;
mod api_command;
pub use api_command::*;
mod bootstrap_password;
pub use bootstrap_password::*;

mod store_pool;
mod task_transition_mutation_response;
mod task_workflow_pool;

mod mutation_id;
mod mutation_response;

mod project_list_query;

mod mcp_project_query;
mod platform_mcp_catalog;
mod platform_mcp_tools;
mod platform_mcp_transport;

mod repository_handlers;

mod mcp_repository_list_query;

mod mcp_repository_query;

mod mcp_repository_registration;

mod mcp_repository_change;

mod repository_mcp_catalog;

mod repository_mcp_tools;

mod mcp_tool_result;

mod oidc_callback_query;
mod oidc_handlers;
mod oidc_start_request;

mod git_callback_query;
mod git_connection_list_query;
mod git_handlers;
mod git_start_request;

mod github_handlers;
mod github_page_query;

mod github_manifest_handlers;

mod git_webhook_handlers;
mod remote_repository_handlers;
mod remote_repository_list_query;

mod task_history_query;
mod work_item_handlers;

mod mcp_backlog_change;
mod mcp_backlog_create;
mod mcp_backlog_get;
mod mcp_backlog_reorder;
mod mcp_task_change;
mod mcp_task_create;
mod mcp_task_get;
mod mcp_task_history;
mod mcp_task_list;
mod mcp_task_transition;
mod mcp_task_type_create;
mod work_item_mcp_catalog;
mod work_item_mcp_tools;

mod mcp_task_approval_get;
mod task_approval_handlers;

mod task_collaboration_handlers;
mod task_comment_list_query;

mod mcp_task_comment_add;
mod mcp_task_comments_list;
mod mcp_task_dependency_change;
mod mcp_task_dependency_create;
mod mcp_task_dependency_list;
mod task_collaboration_mcp_catalog;
mod task_collaboration_mcp_tools;

mod task_template_handlers;
mod task_template_list_query;
mod task_template_versions_query;

mod mcp_schema_definitions;
mod mcp_task_template_apply;
mod mcp_task_template_change;
mod mcp_task_template_create;
mod mcp_task_template_get;
mod mcp_task_template_list;
mod mcp_task_template_publish;
mod mcp_task_template_version_get;
mod mcp_task_template_versions_list;
mod task_template_mcp_catalog;
mod task_template_mcp_tools;

mod mcp_document_create;

mod mcp_document_change;

mod mcp_document_revise;

mod mcp_document_transition;

mod mcp_document_publish;

mod mcp_document_get;

mod mcp_document_version_get;

mod mcp_document_versions_list;

mod mcp_document_export;

mod document_handlers;

mod document_mcp_tools;

mod document_versions_query;

mod document_export_query;

mod document_mcp_catalog;
#[cfg(test)]
mod document_mcp_catalog_tests;

mod mcp_document_list;

mod mcp_document_history;

mod mcp_document_publications_list;

mod document_history_query;

mod document_publications_query;

mod document_catalog_handlers;

mod mcp_context_manifest_create;

mod mcp_context_manifest_get;

mod mcp_context_manifest_status;

mod project_context_handlers;

mod project_context_mcp_tools;

mod project_context_mcp_catalog;

mod mcp_document_link_create;

mod mcp_document_link_remove;

mod mcp_context_package_create;

mod mcp_document_links_list;

mod mcp_task_documents_list;

mod mcp_context_package_get;

mod mcp_context_package_export;

mod traceability_handlers;

mod traceability_mcp_tools;

mod document_link_page_query;

mod traceability_mcp_catalog;

mod approval_request_handlers;
mod approval_request_mcp_catalog;
mod approval_request_mcp_tools;
mod approval_request_page_query;
mod mcp_approval_request_cancel;
mod mcp_approval_request_create;
mod mcp_approval_request_get;
mod mcp_approval_requests_list;

mod document_approval_handlers;
mod document_approval_mcp_catalog;
mod document_approval_mcp_tools;
mod mcp_document_approval_get;
mod mcp_task_type_change;
mod mcp_work_catalog_page;
mod work_catalog_handlers;
mod work_catalog_mcp_catalog;
mod work_catalog_mcp_tools;
mod work_catalog_page_query;

mod mcp_task_move;
mod mcp_task_workflow_migrate;
mod task_lifecycle_handlers;
mod task_lifecycle_mcp_catalog;
mod task_lifecycle_mcp_tools;

mod mcp_headers;
pub use mcp_headers::*;

mod knowledge_contribute_input;
mod knowledge_handlers;
mod knowledge_list_query;
mod knowledge_mcp_catalog;
mod knowledge_mcp_input;
mod knowledge_mcp_tools;
mod knowledge_page_query;
mod knowledge_review_input;
mod knowledge_revise_input;

mod mcp_repository_job_cancel;
mod mcp_repository_job_query;
mod mcp_repository_job_request;
mod repository_job_mcp_catalog;
mod repository_job_mcp_tools;

mod reasoning_settings;
pub use reasoning_settings::ReasoningSettings;
mod reasoning_handlers;
mod reasoning_mcp_tools;
mod reasoning_repository;
mod reasoning_request;

mod reasoning_requirement;
#[cfg(test)]
mod reasoning_tests;

mod requirement_reasoning_context;
#[cfg(test)]
mod requirement_reasoning_tests;

mod evidence_handlers;
mod evidence_mcp_catalog;
mod evidence_mcp_tools;
mod evidence_page_query;
mod mcp_evidence_get;
mod mcp_evidence_list;
mod mcp_evidence_upload;

mod mcp_development_policy_set;
mod mcp_task_work_prepare;
mod mcp_task_work_query;
mod mcp_task_work_report;
mod task_work_mcp_tools;

mod api_cors;
pub use api_cors::*;

mod mcp_project_change;
mod mcp_project_create;
mod project_mutation_mcp_catalog;
mod project_mutation_mcp_tools;

mod pull_request_handlers;
mod pull_request_list_query;

mod pull_request_link_handlers;
mod pull_request_links_query;

mod developer_workflow_handlers;
mod developer_workflow_member_handlers;

mod mcp_developer_workflow_assign;
mod mcp_developer_workflow_query;

mod visible_task_handlers;

mod task_type_catalog_handlers;

mod live_update_handlers;
mod live_update_query;

mod installation_preferences_handlers;

mod installation_preferences_events;

mod organization_logo;
mod organization_logo_input;

mod smtp_mailer;
mod smtp_test_handlers;
mod smtp_test_input;

mod organization_settings_handlers;

mod installation_service_handlers;

mod notification_recipient_handlers;

mod email_access_handlers;
mod email_action_token_input;
mod email_content_handlers;
mod email_template_handlers;
mod member_email_handlers;
#[cfg(test)]
mod organization_logo_tests;
mod password_recovery_complete;
mod password_recovery_request;
mod public_email_access_handlers;

mod developer_rating_handlers;
mod installation_timezone_handlers;
mod labor_calendar_handlers;
mod member_role_handlers;
mod overtime_evidence_handlers;
mod overtime_handlers;

mod feature_handlers;
mod feature_mcp_catalog;
mod feature_mcp_tools;
mod mcp_feature_proposal;
mod mcp_feature_query;

mod member_profile_handlers;
mod member_profile_query;
mod member_work_week_handlers;
mod member_work_week_query;

mod member_avatar_handlers;
mod member_avatar_input;

mod project_profile_handlers;
mod project_profile_query;

mod repository_profile_handlers;
mod repository_profile_query;

mod member_position_handlers;

mod effective_permissions_handlers;

mod task_pool_handlers;
mod task_pool_options_query;
mod task_relation_handlers;

mod mcp_task_pause;
mod mcp_task_take;

mod mcp_task_pool_list;
mod mcp_task_pool_options;

mod mcp_task_relations_get;

mod mcp_task_subtask_create;

mod mcp_task_subtask_change;

mod git_author_handlers;

mod git_author_mcp_catalog;
mod mcp_git_author_query;

mod analysis_profile_handlers;
mod analysis_profile_query;

mod analysis_profile_mcp_catalog;
mod mcp_repository_analysis_profile_get;

mod analysis_model_guide;
mod analysis_profile_authoring;
mod analysis_profile_authoring_mcp_tools;
mod mcp_analysis_model_guide_get;
mod mcp_analysis_profile_package;
mod mcp_analysis_profile_validate;

mod repository_analysis_profile_mcp_tools;

mod navigation_order_handlers;

mod code_review_handlers;
mod code_review_request;

mod personal_mcp_key_handlers;

mod calendar_activity_handlers;

mod task_evidence_handlers;
mod task_evidence_image;
mod task_evidence_upload;

mod mcp_task_knowledge_link;
mod task_knowledge_handlers;
mod task_knowledge_query;

mod mcp_task_project_change;
mod task_project_handlers;

mod identity_gate_handlers;
mod identity_gate_request;
mod totp_exemption_request;

mod browser_session;
mod browser_session_handlers;

mod member_lifecycle_handlers;
mod member_lifecycle_request;
mod user_email_handlers;
mod user_email_request;

mod rating_feedback_handlers;

mod repository_file_handlers;

mod task_document_handlers;
mod task_document_query;
mod task_document_upload;

mod organization_knowledge_handlers;
mod organization_knowledge_query;

mod organization_knowledge_mcp;
mod organization_knowledge_mcp_input;

mod rating_evidence_handlers;
mod rating_evidence_upload;

mod sticker_handlers;

mod task_timeline_handlers;

mod repository_commit_handlers;

mod in_app_notification_handlers;

mod daily_developer_analysis_handlers;

mod knowledge_contribution_handlers;
mod knowledge_contribution_mcp;
mod knowledge_contribution_mcp_input;
mod knowledge_contribution_query;

mod analysis_profile_evolution_input;
mod analysis_profile_evolution_mcp;

#[cfg(test)]
mod knowledge_contribution_tests;

mod task_transition_run_handlers;

mod mcp_task_transition_retry;

mod mcp_feature_complete;
mod mcp_task_attachment_upload;
mod mcp_task_attachments_query;
mod task_attachment_mcp_tools;

mod knowledge_image_handlers;
mod knowledge_image_upload;

mod mcp_task_work_log;
mod task_work_log_handlers;

mod general_task_completion_handlers;

mod mcp_task_release;
mod task_release_handlers;

mod mcp_task_commit;
mod task_commit_handlers;

mod branch_protection_handlers;

mod personal_work_block_handlers;

mod task_workflow_migration_handlers;

mod service_request_handlers;

mod service_request_mcp_tools;

mod knowledge_review_handlers;
mod knowledge_review_mcp_tools;
mod product_handlers;
