use crate::{
    PlatformStore, audit_writer::AuditWriter,
    developer_workflow_authority::DeveloperWorkflowAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct DeveloperWorkflowStore;
impl DeveloperWorkflowStore {
    pub fn directory(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<DeveloperDirectoryPage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|id| id.is_empty() || id.len() > 128) {
            return Err(PlatformError::Validation("invalid developer page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DeveloperWorkflowAuthority::require(&mut tx, ctx, false)?;
        let rows=tx.query("SELECT jsonb_build_object('id',a.id,'name',a.name,'workflow_id',d.workflow_id,'workflow_version_id',d.workflow_version_id,'assignment_revision',COALESCE(d.revision,0),'role',(SELECT role FROM actor_roles WHERE actor_id=a.id),'workflows',COALESCE((SELECT jsonb_agg(to_jsonb(m)||jsonb_build_object('actor_name',a.name) ORDER BY m.workflow_id) FROM developer_workflows m WHERE m.actor_id=a.id),'[]'::jsonb)) FROM actors a LEFT JOIN LATERAL (SELECT m.* FROM developer_workflows m WHERE m.actor_id=a.id AND NOT EXISTS(SELECT 1 FROM developer_workflows other WHERE other.actor_id=a.id AND other.workflow_id<>m.workflow_id)) d ON true WHERE a.active AND ($1::text IS NULL OR a.id>$1) ORDER BY a.id LIMIT $2",&[&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<DeveloperDirectoryEntry>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DeveloperDirectoryPage { items, next_after })
    }
    pub fn members(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        workflow: &str,
    ) -> Result<Vec<DeveloperWorkflowAssignment>, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if !crate::workflow_read_authority::WorkflowReadAuthority::scope(&mut tx, ctx)?
            .permits(workflow)
        {
            return Err(PlatformError::NotFound);
        }
        tx.query_opt(
            "SELECT id FROM workflows WHERE id=$1 AND project_id IS NULL",
            &[&workflow],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let rows=tx.query("SELECT to_jsonb(d)||jsonb_build_object('actor_name',a.name) FROM developer_workflows d JOIN actors a ON a.id=d.actor_id WHERE d.workflow_id=$1 AND a.active ORDER BY a.name,d.actor_id",&[&workflow]).map_err(PlatformStore::database_error)?;
        let result = rows
            .into_iter()
            .map(|row| PlatformStore::decode(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn list(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<Vec<DeveloperWorkflowAssignment>, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if actor == ctx.actor_id() {
            crate::workflow_read_authority::WorkflowReadAuthority::scope(&mut tx, ctx)?;
        } else {
            DeveloperWorkflowAuthority::require(&mut tx, ctx, false)?;
        }
        let rows=tx.query("SELECT to_jsonb(d)||jsonb_build_object('actor_name',a.name) FROM developer_workflows d JOIN actors a ON a.id=d.actor_id WHERE d.actor_id=$1 ORDER BY d.workflow_id",&[&actor]).map_err(PlatformStore::database_error)?;
        let result = rows
            .into_iter()
            .map(|row| PlatformStore::decode(row.get::<_, Value>(0)))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<Option<DeveloperWorkflowAssignment>, PlatformError> {
        let mut assignments = Self::list(store, ctx, actor)?;
        if assignments.len() > 1 {
            return Err(PlatformError::Validation(
                "multiple workflows assigned; list memberships and select a workflow",
            ));
        }
        Ok(assignments.pop())
    }
    pub fn assign(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        actor: &str,
        input: &DeveloperWorkflowChange,
    ) -> Result<DeveloperWorkflowAssignment, PlatformError> {
        if input.expected_revision < 0 {
            return Err(PlatformError::Validation(
                "invalid developer workflow revision",
            ));
        }
        let reason = DomainText::optional(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DeveloperWorkflowAuthority::require(&mut tx, ctx, true)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "developer.workflow_assign",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active FOR UPDATE",
            &[&actor],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let role: String = tx
            .query_opt(
                "SELECT role FROM actor_roles WHERE actor_id=$1 FOR SHARE",
                &[&actor],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Validation("member needs a team role"))?
            .get(0);
        let single = role == "developer";
        let flow:WorkflowVersion=PlatformStore::decode(tx.query_opt("SELECT to_jsonb(v) FROM workflow_versions v JOIN workflows w ON w.id=v.workflow_id WHERE v.id=$1 AND w.project_id IS NULL AND NOT w.archived FOR SHARE OF w",&[&input.workflow_version_id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0))?;
        let existing=tx.query_opt("SELECT revision FROM developer_workflows WHERE actor_id=$1 AND ($2 OR workflow_id=$3) FOR UPDATE",&[&actor,&single,&flow.workflow_id]).map_err(PlatformStore::database_error)?;
        if existing.map(|row| row.get::<_, i64>(0)).unwrap_or(0) != input.expected_revision {
            return Err(PlatformError::Conflict(
                "developer workflow revision changed",
            ));
        }
        if single {
            if tx.query_one("SELECT EXISTS(SELECT 1 FROM tasks WHERE assignee_id=$1 AND NOT archived AND lifecycle IN ('active','paused') AND workflow_id<>$2)",&[&actor,&flow.workflow_id]).map_err(PlatformStore::database_error)?.get::<_,bool>(0) { return Err(PlatformError::Conflict("reassign existing tasks before changing developer workflow")); }
            tx.execute(
                "DELETE FROM developer_workflows WHERE actor_id=$1 AND workflow_id<>$2",
                &[&actor, &flow.workflow_id],
            )
            .map_err(PlatformStore::database_error)?;
        }
        // The counter survives membership removal. Keep revision zero as the public
        // precondition for absence without reusing a previous assignment revision.
        let revision: i64 = tx.query_one(
            "INSERT INTO developer_workflow_revisions(actor_id,revision) VALUES($1,1) ON CONFLICT(actor_id) DO UPDATE SET revision=developer_workflow_revisions.revision+1 RETURNING revision",
            &[&actor],
        ).map_err(PlatformStore::database_error)?.get(0);
        tx.execute("INSERT INTO developer_workflows(actor_id,workflow_id,workflow_version_id,assigned_by,revision) VALUES($1,$2,$3,$4,$5) ON CONFLICT(actor_id,workflow_id) DO UPDATE SET workflow_version_id=excluded.workflow_version_id,revision=excluded.revision,assigned_by=excluded.assigned_by,updated_at=clock_timestamp()",&[&actor,&flow.workflow_id,&flow.id,&ctx.actor_id(),&revision]).map_err(PlatformStore::database_error)?;
        let value:Value=tx.query_one("SELECT to_jsonb(d)||jsonb_build_object('actor_name',a.name) FROM developer_workflows d JOIN actors a ON a.id=d.actor_id WHERE d.actor_id=$1 AND d.workflow_id=$2",&[&actor,&flow.workflow_id]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "developer.workflow_assigned",
            Some(actor),
            json!({"workflow_id":flow.workflow_id,"workflow_version_id":flow.id,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn remove(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        actor: &str,
        input: &DeveloperWorkflowRemoval,
    ) -> Result<(), PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("invalid membership revision"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DeveloperWorkflowAuthority::require(&mut tx, ctx, true)?;
        if ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "developer.workflow_remove",
        )?
        .is_some()
        {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        tx.query_opt("SELECT id FROM actors WHERE id=$1 FOR UPDATE", &[&actor])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let row = tx
            .query_opt(
                "SELECT revision,workflow_id FROM developer_workflows WHERE actor_id=$1 AND revision=$2 FOR UPDATE",
                &[&actor,&input.expected_revision],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Conflict("developer workflow revision changed"))?;
        if row.get::<_, i64>(0) != input.expected_revision {
            return Err(PlatformError::Conflict(
                "developer workflow revision changed",
            ));
        }
        let workflow: String = row.get(1);
        if tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE assignee_id=$1 AND workflow_id=$2 AND NOT archived AND lifecycle IN ('active','paused'))",
                &[&actor,&workflow],
            )
            .map_err(PlatformStore::database_error)?
            .get::<_, bool>(0)
        {
            return Err(PlatformError::Conflict(
                "archive or reassign existing tasks before removing developer workflow",
            ));
        }
        tx.execute(
            "DELETE FROM developer_workflows WHERE actor_id=$1 AND workflow_id=$2",
            &[&actor, &workflow],
        )
        .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "developer.workflow_removed",
            Some(actor),
            json!({"workflow_id":workflow,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(())
    }
}
