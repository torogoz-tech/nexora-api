use crate::PlatformStore;
use nexora_platform::*;
pub(crate) struct TaskPurposePolicy;
impl TaskPurposePolicy {
    pub fn validate(
        tx: &mut postgres::Transaction<'_>,
        project: Option<&str>,
        code: &str,
        requirement: Option<&str>,
        feature: Option<&str>,
        workflow: Option<&str>,
        assignee: Option<&str>,
    ) -> Result<TaskPurpose, PlatformError> {
        if requirement.is_some() && feature.is_some() {
            return Err(PlatformError::Validation(
                "select at most one primary requirement or feature",
            ));
        }
        let purpose: String = tx
            .query_opt(
                "SELECT purpose FROM task_type_catalog WHERE code=$1 AND NOT archived FOR SHARE",
                &[&code],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Validation(
                "catalog task type is unknown or archived",
            ))?
            .get(0);
        let purpose: TaskPurpose = PlatformStore::decode(serde_json::Value::String(purpose))?;
        if purpose == TaskPurpose::ProductDesign && feature.is_some()
            || purpose == TaskPurpose::Implementation && requirement.is_some()
        {
            return Err(PlatformError::Validation(
                "primary knowledge link does not match task purpose",
            ));
        }
        if let Some(id) = requirement {
            tx.query_opt("SELECT id FROM project_documents WHERE id=$1 AND project_id=$2 AND kind='requirement' AND NOT archived FOR SHARE", &[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Validation("primary requirement must belong to the task project"))?;
        }
        if let Some(id) = feature {
            tx.query_opt("SELECT id FROM project_features WHERE id=$1 AND project_id=$2 AND NOT archived FOR SHARE", &[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Validation("primary feature must belong to the task project"))?;
        }
        if purpose == TaskPurpose::General && workflow.is_some() {
            return Err(PlatformError::Validation(
                "general tasks do not use a workflow",
            ));
        }
        if let Some(id) = workflow {
            tx.query_opt("SELECT id FROM workflows WHERE id=$1 AND NOT archived AND latest_version_id IS NOT NULL AND resource_kinds @> '[\"task\"]'::jsonb AND (project_id IS NULL OR project_id=$2) FOR SHARE", &[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Validation("entry workflow must be active and published"))?;
            if let Some(actor) = assignee {
                crate::developer_task_policy::DeveloperTaskPolicy::assigned_version(tx, actor, id)?;
            }
        }
        Ok(purpose)
    }
}
