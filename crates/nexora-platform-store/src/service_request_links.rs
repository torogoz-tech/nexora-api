use crate::{
    PlatformStore, platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    service_request_policy::ServiceRequestPolicy as Policy,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn link_service_request(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::platform_authority::PlatformAuthority::require(
            &mut tx,
            ctx,
            Capability::RequestsWrite,
            None,
        )?;
        let role = Policy::visible(&mut tx, ctx, id, true)?;
        if !Policy::internal(&role) {
            return Err(PlatformError::Forbidden);
        }
        let kind = Policy::text(input, "kind", 20)?;
        let target = Policy::text(input, "target_id", 150)?;
        let project = match kind {
            "task" => {
                TaskScopeAuthority::task(&mut tx, ctx, target, Capability::TasksWrite)?.project_id
            }
            "feature" => {
                let project: String = tx
                    .query_opt(
                        "SELECT project_id FROM project_features WHERE id=$1 AND NOT archived",
                        &[&target],
                    )
                    .map_err(Self::database_error)?
                    .ok_or(PlatformError::NotFound)?
                    .get(0);
                PlatformAuthority::require(
                    &mut tx,
                    ctx,
                    Capability::DocumentsWrite,
                    Some(&project),
                )?;
                Some(project)
            }
            _ => return Err(PlatformError::Validation("invalid request link")),
        };
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "service_request.link",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let revision = input["expected_revision"]
            .as_i64()
            .ok_or(PlatformError::Validation("revision required"))?;
        if tx.execute("UPDATE service_requests SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2",&[&id,&revision]).map_err(Self::database_error)?!=1{return Err(PlatformError::Conflict("request changed"))}
        if input["remove"].as_bool() == Some(true) {
            tx.execute("DELETE FROM service_request_links WHERE request_id=$1 AND kind=$2 AND target_id=$3",&[&id,&kind,&target]).map_err(Self::database_error)?;
        } else {
            tx.execute("INSERT INTO service_request_links(request_id,kind,target_id,project_id,linked_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",&[&id,&kind,&target,&project,&ctx.actor_id()]).map_err(Self::database_error)?;
        }
        tx.execute("INSERT INTO service_request_history(request_id,actor_id,action,detail) VALUES($1,$2,'linked',$3)",&[&id,&ctx.actor_id(),&input]).map_err(Self::database_error)?;
        let result = json!({"id":id,"revision":revision+1});
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn linked_service_requests(
        &mut self,
        ctx: &AuthContext,
        kind: &str,
        target: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let role = Policy::role(&mut tx, ctx)?;
        if !Policy::internal(&role) {
            return Err(PlatformError::Forbidden);
        }
        match kind {
            "task" => {
                TaskScopeAuthority::task(&mut tx, ctx, target, Capability::TasksRead)?;
            }
            "feature" => {
                let project: String = tx
                    .query_opt(
                        "SELECT project_id FROM project_features WHERE id=$1",
                        &[&target],
                    )
                    .map_err(Self::database_error)?
                    .ok_or(PlatformError::NotFound)?
                    .get(0);
                PlatformAuthority::require(
                    &mut tx,
                    ctx,
                    Capability::DocumentsRead,
                    Some(&project),
                )?;
            }
            _ => return Err(PlatformError::Validation("invalid link kind")),
        }
        let items=tx.query("SELECT to_jsonb(r) FROM service_requests r JOIN service_request_links l ON l.request_id=r.id WHERE l.kind=$1 AND l.target_id=$2 ORDER BY r.created_at DESC",&[&kind,&target]).map_err(Self::database_error)?.iter().map(|r|r.get::<_,Value>(0)).collect::<Vec<_>>();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items}))
    }
}
