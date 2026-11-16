use crate::receipt_writer::ReceiptWriter;
use crate::{PlatformStore, audit_writer::AuditWriter};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct GrantStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GrantStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn grant(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        capability: Capability,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ActorsManage, None)?;
        ctx.require(capability, project)?;
        self.validate_target(actor, project)?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        tx.query_opt("SELECT id FROM actors WHERE id=$1 FOR UPDATE", &[&actor])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        if tx
            .query_opt(
                "SELECT id FROM actors WHERE id=$1 AND kind='human'",
                &[&actor],
            )
            .map_err(PlatformStore::database_error)?
            .is_some()
        {
            return Err(PlatformError::Forbidden);
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "grant.add",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            let _: () = PlatformStore::decode(value)?;
            return Ok(());
        }

        let n=tx.execute("INSERT INTO actor_grants(actor_id,capability,project_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&actor,&capability.as_str(),&project]).map_err(PlatformStore::database_error)?;
        if n > 0 {
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "actor.grant_created",
                Some(actor),
                json!({"capability":capability,"project_id":project}),
            )?;
        }
        ReceiptWriter::save(&mut tx, request.as_ref(), &serde_json::Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn revoke(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        capability: Capability,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ActorsManage, None)?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        tx.query_opt("SELECT id FROM actors WHERE id=$1 FOR UPDATE", &[&actor])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        if tx
            .query_opt(
                "SELECT id FROM actors WHERE id=$1 AND kind='human'",
                &[&actor],
            )
            .map_err(PlatformStore::database_error)?
            .is_some()
        {
            return Err(PlatformError::Forbidden);
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "grant.revoke",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            let _: () = PlatformStore::decode(value)?;
            return Ok(());
        }

        let n=tx.execute("DELETE FROM actor_grants WHERE actor_id=$1 AND capability=$2 AND project_id IS NOT DISTINCT FROM $3",&[&actor,&capability.as_str(),&project]).map_err(PlatformStore::database_error)?;
        if n > 0 {
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "actor.grant_revoked",
                Some(actor),
                json!({"capability":capability,"project_id":project}),
            )?;
        }
        ReceiptWriter::save(&mut tx, request.as_ref(), &serde_json::Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    fn validate_target(&mut self, actor: &str, project: Option<&str>) -> Result<(), PlatformError> {
        if self
            .store
            .client
            .query_opt("SELECT id FROM actors WHERE id=$1 AND active", &[&actor])
            .map_err(PlatformStore::database_error)?
            .is_none()
        {
            return Err(PlatformError::NotFound);
        }
        if let Some(project) = project
            && self
                .store
                .client
                .query_opt(
                    "SELECT id FROM projects WHERE id=$1 AND NOT archived",
                    &[&project],
                )
                .map_err(PlatformStore::database_error)?
                .is_none()
        {
            return Err(PlatformError::NotFound);
        }
        Ok(())
    }
}
