use crate::PlatformStore;
use nexora_platform::*;
use postgres::Transaction;
use serde_json::Value;

pub(crate) struct TaskPolicyStore;
impl TaskPolicyStore {
    pub fn flow(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
        fresh: bool,
    ) -> Result<WorkflowVersion, PlatformError> {
        crate::task_pool_workflow::TaskPoolWorkflow::flow(tx, Some(project), id, fresh)
    }

    pub fn actor(tx: &mut Transaction<'_>, id: &str) -> Result<(), PlatformError> {
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active FOR SHARE",
            &[&id],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Validation(
            "assigned actor is unknown or inactive",
        ))?;
        Ok(())
    }
    pub fn task(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
    ) -> Result<WorkTask, PlatformError> {
        Self::scoped_task(tx, Some(project), id)
    }
    pub fn scoped_task(
        tx: &mut Transaction<'_>,
        project: Option<&str>,
        id: &str,
    ) -> Result<WorkTask, PlatformError> {
        if let Some(project) = project {
            crate::task_dependency_lock::TaskDependencyLock::shared(tx, project)?;
        }
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(t) FROM tasks t WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 FOR UPDATE",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        PlatformStore::decode(value)
    }
    pub fn backlog(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
    ) -> Result<Backlog, PlatformError> {
        crate::task_dependency_lock::TaskDependencyLock::shared(tx, project)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(b) FROM backlogs b WHERE id=$1 AND project_id=$2 FOR UPDATE",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        PlatformStore::decode(value)
    }
    pub fn context(value: &str) -> Result<String, PlatformError> {
        if value.trim().is_empty() {
            Ok(String::new())
        } else {
            DomainText::required(value, 8000)
        }
    }
    pub fn fields(fields: &serde_json::Map<String, Value>) -> Result<Value, PlatformError> {
        if fields.len() > 64
            || fields.keys().any(|key| {
                key.is_empty()
                    || key.len() > 64
                    || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            || serde_json::to_vec(fields)
                .map_err(|_| PlatformError::Storage)?
                .len()
                > 16384
        {
            return Err(PlatformError::Validation("invalid task custom fields"));
        }
        serde_json::to_value(fields).map_err(|_| PlatformError::Storage)
    }
    pub fn normalize_fields(
        tx: &mut Transaction<'_>,
        fields: &serde_json::Map<String, Value>,
    ) -> Result<Value, PlatformError> {
        let input = Self::fields(fields)?;
        let row = tx
            .query_one(
                "SELECT $1::jsonb,octet_length(($1::jsonb)::text)",
                &[&input],
            )
            .map_err(PlatformStore::database_error)?;
        if row.get::<_, i32>(1) > 32768 {
            return Err(PlatformError::Validation("stored task fields exceed limit"));
        }
        let value: Value = row.get(0);
        Self::fields(value.as_object().ok_or(PlatformError::Storage)?)?;
        Ok(value)
    }
    pub fn kind(tx: &mut Transaction<'_>, _project: &str, code: &str) -> Result<(), PlatformError> {
        tx.query_opt(
            "SELECT code FROM task_type_catalog WHERE code=$1 AND NOT archived FOR SHARE",
            &[&code],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Validation(
            "catalog task type is unknown or archived",
        ))?;
        Ok(())
    }
}
