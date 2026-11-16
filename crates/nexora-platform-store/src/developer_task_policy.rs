use crate::PlatformStore;
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct DeveloperTaskPolicy;
impl DeveloperTaskPolicy {
    pub fn version(
        tx: &mut Transaction<'_>,
        actor: &str,
        fresh: bool,
    ) -> Result<Option<String>, PlatformError> {
        Self::select(tx, actor, fresh, None, false)
    }
    pub fn assigned_version(
        tx: &mut Transaction<'_>,
        actor: &str,
        workflow: &str,
    ) -> Result<Option<String>, PlatformError> {
        Self::select(tx, actor, true, Some(workflow), false)
    }
    pub fn take_version(
        tx: &mut Transaction<'_>,
        actor: &str,
        workflow: Option<&str>,
    ) -> Result<Option<String>, PlatformError> {
        Self::select(tx, actor, true, workflow, true)
    }
    fn select(
        tx: &mut Transaction<'_>,
        actor: &str,
        fresh: bool,
        workflow: Option<&str>,
        taking: bool,
    ) -> Result<Option<String>, PlatformError> {
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND (NOT $2 OR active) FOR UPDATE",
            &[&actor, &fresh],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Validation(
            "assigned member is unknown or inactive",
        ))?;
        if taking {
            let role: String = tx
                .query_opt(
                    "SELECT role FROM actor_roles WHERE actor_id=$1 FOR SHARE",
                    &[&actor],
                )
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::Forbidden)?
                .get(0);
            if role != "developer" && workflow.is_none() {
                return Err(PlatformError::Validation(
                    "select an assigned workflow before taking a task",
                ));
            }
        }
        let rows=tx.query("SELECT d.workflow_version_id FROM developer_workflows d JOIN workflows w ON w.id=d.workflow_id WHERE d.actor_id=$1 AND (NOT $2 OR NOT w.archived) AND ($3::text IS NULL OR d.workflow_id=$3) FOR SHARE OF d,w",&[&actor,&fresh,&workflow]).map_err(PlatformStore::database_error)?;
        if rows.len() > 1 {
            return Err(PlatformError::Validation("select an assigned workflow"));
        }
        if workflow.is_some() && rows.is_empty() {
            return Err(PlatformError::Validation(
                "workflow must be assigned to the member and active",
            ));
        }
        Ok(rows.first().map(|row| row.get(0)))
    }
}
