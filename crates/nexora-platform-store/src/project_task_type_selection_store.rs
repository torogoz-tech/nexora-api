use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct ProjectTaskTypeSelectionStore;
impl ProjectTaskTypeSelectionStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<ProjectTaskTypeSelection, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        let codes = tx
            .query(
                "SELECT code FROM task_types WHERE project_id=$1 AND NOT archived ORDER BY code",
                &[&project],
            )
            .map_err(PlatformStore::database_error)?
            .into_iter()
            .map(|r| r.get(0))
            .collect();
        let revision = tx
            .query_opt(
                "SELECT revision FROM project_task_type_settings WHERE project_id=$1",
                &[&project],
            )
            .map_err(PlatformStore::database_error)?
            .map(|r| r.get(0))
            .unwrap_or(0);
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(ProjectTaskTypeSelection { codes, revision })
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectTaskTypeSelectionChange,
    ) -> Result<ProjectTaskTypeSelection, PlatformError> {
        if input.expected_revision < 0
            || input.codes.len() > 500
            || input.codes.iter().any(|v| v.is_empty() || v.len() > 64)
            || input
                .codes
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != input.codes.len()
        {
            return Err(PlatformError::Validation("invalid task type selection"));
        }
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "project.task_types.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        // Lock shared catalog rows before selection rows, same order as global edits.
        // Include removed types in the lock set, before locking project settings.
        // Catalog metadata updates touch those same project rows and their revision trigger.
        tx.query("SELECT code FROM task_type_catalog c WHERE code=ANY($1) OR EXISTS(SELECT 1 FROM task_types t WHERE t.project_id=$2 AND t.code=c.code) ORDER BY code FOR SHARE OF c", &[&input.codes,&project]).map_err(PlatformStore::database_error)?;
        let rows=tx.query("SELECT code FROM task_type_catalog c WHERE code=ANY($1) AND (NOT archived OR EXISTS(SELECT 1 FROM task_types t WHERE t.project_id=$2 AND t.code=c.code AND NOT t.archived)) ORDER BY code FOR SHARE OF c",&[&input.codes, &project]).map_err(PlatformStore::database_error)?;
        if rows.len() != input.codes.len() {
            return Err(PlatformError::Validation(
                "selected task type is unknown or archived",
            ));
        }
        tx.execute("INSERT INTO project_task_type_settings(project_id,revision) VALUES($1,0) ON CONFLICT DO NOTHING",&[&project]).map_err(PlatformStore::database_error)?;
        let revision: i64 = tx
            .query_one(
                "SELECT revision FROM project_task_type_settings WHERE project_id=$1 FOR UPDATE",
                &[&project],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "project task type selection changed",
            ));
        }
        tx.execute("UPDATE task_types SET archived=true,revision=revision+1 WHERE project_id=$1 AND NOT archived AND NOT(code=ANY($2))",&[&project,&input.codes]).map_err(PlatformStore::database_error)?;
        tx.execute("INSERT INTO task_types(project_id,code,name,description,color) SELECT $1,code,name,description,color FROM task_type_catalog WHERE code=ANY($2) ON CONFLICT(project_id,code) DO UPDATE SET archived=false,name=EXCLUDED.name,description=EXCLUDED.description,color=EXCLUDED.color,revision=task_types.revision+1",&[&project,&input.codes]).map_err(PlatformStore::database_error)?;
        let revision=tx.query_one("UPDATE project_task_type_settings SET revision=revision+1 WHERE project_id=$1 RETURNING revision",&[&project]).map_err(PlatformStore::database_error)?.get(0);
        let mut codes = input.codes.clone();
        codes.sort();
        let result = ProjectTaskTypeSelection { codes, revision };
        let value = json!(result);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "project.task_types.changed",
            Some(project),
            json!({"project_id":project,"selection":result}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
