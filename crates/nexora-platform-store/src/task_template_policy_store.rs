use crate::{PlatformStore, task_template_checksum::TaskTemplateChecksum};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::Value;

pub(crate) struct TaskTemplatePolicyStore;
impl TaskTemplatePolicyStore {
    pub fn definition(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
    ) -> Result<TaskTemplate, PlatformError> {
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(t) FROM task_templates t WHERE id=$1 AND project_id=$2 FOR UPDATE",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        PlatformStore::decode(value)
    }
    pub fn applicable_version(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
    ) -> Result<TaskTemplateVersion, PlatformError> {
        let value:Value=tx.query_opt("SELECT to_jsonb(v) FROM task_template_versions v JOIN task_templates t ON t.id=v.template_id AND t.project_id=v.project_id WHERE v.id=$1 AND v.project_id=$2 AND NOT t.archived AND v.valid_from<=clock_timestamp() AND (v.valid_until IS NULL OR v.valid_until>clock_timestamp()) FOR SHARE OF t",&[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("template version is unavailable"))?.get(0);
        let version: TaskTemplateVersion = PlatformStore::decode(value)?;
        if TaskTemplateChecksum::calculate(&version)? != version.checksum {
            return Err(PlatformError::Conflict("template checksum changed"));
        }
        Ok(version)
    }
}
