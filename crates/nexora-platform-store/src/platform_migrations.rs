pub(crate) struct PlatformMigrations;
impl PlatformMigrations {
    pub fn all() -> &'static [(i32, &'static str)] {
        &[
            (1, include_str!("../migrations/0001_platform.sql")),
            (2, include_str!("../migrations/0002_request_integrity.sql")),
            (
                3,
                include_str!("../migrations/0003_repository_registry.sql"),
            ),
            (4, include_str!("../migrations/0004_corporate_identity.sql")),
            (5, include_str!("../migrations/0005_git_connections.sql")),
            (6, include_str!("../migrations/0006_github_app.sql")),
            (7, include_str!("../migrations/0007_github_manifest.sql")),
            (8, include_str!("../migrations/0008_git_webhooks.sql")),
            (
                9,
                include_str!("../migrations/0009_provider_credential_revisions.sql"),
            ),
            (10, include_str!("../migrations/0010_repository_jobs.sql")),
            (
                11,
                include_str!("../migrations/0011_repository_job_leases.sql"),
            ),
            (
                12,
                include_str!("../migrations/0012_repository_bindings.sql"),
            ),
            (13, include_str!("../migrations/0013_workflows.sql")),
            (14, include_str!("../migrations/0014_work_items.sql")),
            (15, include_str!("../migrations/0015_task_approvals.sql")),
            (
                16,
                include_str!("../migrations/0016_task_collaboration.sql"),
            ),
            (17, include_str!("../migrations/0017_task_templates.sql")),
            (18, include_str!("../migrations/0018_project_documents.sql")),
            (
                19,
                include_str!("../migrations/0019_document_catalog_indexes.sql"),
            ),
            (20, include_str!("../migrations/0020_project_context.sql")),
            (
                21,
                include_str!("../migrations/0021_document_traceability.sql"),
            ),
            (22, include_str!("../migrations/0022_approval_requests.sql")),
            (
                23,
                include_str!("../migrations/0023_local_snapshot_sources.sql"),
            ),
            (24, include_str!("../migrations/0024_evidence.sql")),
            (25, include_str!("../migrations/0025_task_work.sql")),
            (26, include_str!("../migrations/0026_webhook_analysis.sql")),
            (27, include_str!("../migrations/0027_entity_colors.sql")),
            (
                28,
                include_str!("../migrations/0028_pull_request_links.sql"),
            ),
            (
                29,
                include_str!("../migrations/0029_optional_workflow_purpose.sql"),
            ),
            (
                30,
                include_str!("../migrations/0030_developer_workflows.sql"),
            ),
            (31, include_str!("../migrations/0031_task_type_catalog.sql")),
            (32, include_str!("../migrations/0032_live_updates.sql")),
            (
                33,
                include_str!("../migrations/0033_task_type_icons_and_seed.sql"),
            ),
            (
                34,
                include_str!("../migrations/0034_installation_language.sql"),
            ),
            (35, include_str!("../migrations/0035_general_settings.sql")),
            (
                36,
                include_str!("../migrations/0036_email_notifications.sql"),
            ),
            (37, include_str!("../migrations/0037_email_template.sql")),
            (38, include_str!("../migrations/0038_email_contents.sql")),
            (
                39,
                include_str!("../migrations/0039_email_layout_subject.sql"),
            ),
            (
                40,
                include_str!("../migrations/0040_member_email_flows.sql"),
            ),
            (41, include_str!("../migrations/0041_member_roles.sql")),
            (
                42,
                include_str!("../migrations/0042_requirement_features.sql"),
            ),
            (43, include_str!("../migrations/0043_member_avatars.sql")),
            (44, include_str!("../migrations/0044_labor_calendar.sql")),
            (45, include_str!("../migrations/0045_developer_ratings.sql")),
            (
                46,
                include_str!("../migrations/0046_working_hours_overtime.sql"),
            ),
            (47, include_str!("../migrations/0047_overtime_evidence.sql")),
            (48, include_str!("../migrations/0048_workflow_icons.sql")),
            (
                49,
                include_str!("../migrations/0049_workflow_business_icons.sql"),
            ),
            (
                50,
                include_str!("../migrations/0050_labor_calendar_country_holiday_kind.sql"),
            ),
            (
                51,
                include_str!("../migrations/0051_technical_leads_member_positions.sql"),
            ),
            (52, include_str!("../migrations/0052_task_pool.sql")),
            (53, include_str!("../migrations/0053_task_relations.sql")),
            (
                54,
                include_str!("../migrations/0054_git_author_aliases.sql"),
            ),
            (55, include_str!("../migrations/0055_analysis_profiles.sql")),
            (
                56,
                include_str!("../migrations/0056_developer_workflow_revisions.sql"),
            ),
            (
                57,
                include_str!("../migrations/0057_multi_analysis_profiles.sql"),
            ),
            (58, include_str!("../migrations/0058_navigation_order.sql")),
            (59, include_str!("../migrations/0059_personal_mcp_keys.sql")),
            (
                60,
                include_str!("../migrations/0060_knowledge_document_kinds.sql"),
            ),
            (
                61,
                include_str!("../migrations/0061_pull_request_reviews.sql"),
            ),
            (
                62,
                include_str!("../migrations/0062_team_calendar_activities.sql"),
            ),
            (63, include_str!("../migrations/0063_task_evidence.sql")),
            (
                64,
                include_str!("../migrations/0064_task_project_change.sql"),
            ),
            (65, include_str!("../migrations/0065_identity_gates.sql")),
            (
                66,
                include_str!("../migrations/0066_manager_configuration.sql"),
            ),
            (
                67,
                include_str!("../migrations/0067_self_service_email.sql"),
            ),
            (68, include_str!("../migrations/0068_member_archival.sql")),
            (69, include_str!("../migrations/0069_workflow_color.sql")),
            (
                70,
                include_str!("../migrations/0070_calendar_attendance.sql"),
            ),
            (71, include_str!("../migrations/0071_rating_feedback.sql")),
            (72, include_str!("../migrations/0072_task_documents.sql")),
            (
                73,
                include_str!("../migrations/0073_organization_knowledge.sql"),
            ),
            (
                74,
                include_str!("../migrations/0074_technical_lead_catalog_read.sql"),
            ),
            (75, include_str!("../migrations/0075_rating_evidence.sql")),
            (
                76,
                include_str!("../migrations/0076_organization_stickers.sql"),
            ),
            (
                77,
                include_str!("../migrations/0077_repository_commit_import.sql"),
            ),
            (
                78,
                include_str!("../migrations/0078_github_webhook_health.sql"),
            ),
            (
                79,
                include_str!("../migrations/0079_git_commit_details.sql"),
            ),
            (
                80,
                include_str!("../migrations/0080_workflow_assignments.sql"),
            ),
            (
                81,
                include_str!("../migrations/0081_git_author_archives.sql"),
            ),
            (82, include_str!("../migrations/0082_global_task_types.sql")),
            (
                83,
                include_str!("../migrations/0083_projectless_rating_evidence.sql"),
            ),
            (
                84,
                include_str!("../migrations/0084_multiple_member_workflows.sql"),
            ),
            (
                85,
                include_str!("../migrations/0085_in_app_notifications.sql"),
            ),
            (
                86,
                include_str!("../migrations/0086_daily_developer_analysis.sql"),
            ),
            (
                87,
                include_str!("../migrations/0087_knowledge_contributions.sql"),
            ),
            (
                88,
                include_str!("../migrations/0088_workflow_transition_runs.sql"),
            ),
            (
                89,
                include_str!("../migrations/0089_organization_git_author_aliases.sql"),
            ),
            (
                90,
                include_str!("../migrations/0090_task_acceptance_criteria.sql"),
            ),
            (
                91,
                include_str!("../migrations/0091_knowledge_without_workflows.sql"),
            ),
            (92, include_str!("../migrations/0092_task_pause.sql")),
            (
                93,
                include_str!("../migrations/0093_task_execution_and_feature_completion.sql"),
            ),
            (
                94,
                include_str!("../migrations/0094_personal_mcp_live_access.sql"),
            ),
            (95, include_str!("../migrations/0095_knowledge_images.sql")),
            (
                96,
                include_str!("../migrations/0096_manual_task_work_logs.sql"),
            ),
            (97, include_str!("../migrations/0097_task_purposes.sql")),
            (
                98,
                include_str!("../migrations/0098_task_pre_pr_reviews.sql"),
            ),
            (99, include_str!("../migrations/0099_task_stages.sql")),
            (
                100,
                include_str!("../migrations/0100_selected_task_releases.sql"),
            ),
            (
                101,
                include_str!("../migrations/0101_contributor_ratings.sql"),
            ),
            (
                103,
                include_str!("../migrations/0103_selective_release_composition.sql"),
            ),
            (
                104,
                include_str!("../migrations/0104_repository_branch_protection.sql"),
            ),
            (
                105,
                include_str!("../migrations/0105_calendar_work_blocks.sql"),
            ),
            (106, include_str!("../migrations/0106_service_requests.sql")),
            (
                107,
                include_str!("../migrations/0107_products_knowledge_reviews.sql"),
            ),
            (108, include_str!("../migrations/0108_request_sla.sql")),
            (
                109,
                include_str!("../migrations/0109_request_type_appearance.sql"),
            ),
            (110, include_str!("../migrations/0110_knowledge_rag.sql")),
            (111, include_str!("../migrations/0111_embedding_model.sql")),
            (
                112,
                include_str!("../migrations/0112_code_snapshot_embeddings.sql"),
            ),
            (
                113,
                include_str!("../migrations/0113_pgvector_ann_indexes.sql"),
            ),
            (
                114,
                include_str!("../migrations/0114_knowledge_processing_leases.sql"),
            ),
            (115, include_str!("../migrations/0115_passkeys.sql")),
            (
                116,
                include_str!("../migrations/0116_primary_auth_method.sql"),
            ),
            (
                117,
                include_str!("../migrations/0117_permanent_personal_mcp_keys.sql"),
            ),
        ]
    }
}
