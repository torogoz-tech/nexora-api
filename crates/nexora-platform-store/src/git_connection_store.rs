use crate::{PlatformStore, audit_writer::AuditWriter, receipt_writer::ReceiptWriter};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GitConnectionStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GitConnectionStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn list(&mut self, ctx: &AuthContext) -> Result<Vec<GitConnection>, PlatformError> {
        self.page(ctx, None, None, 100).map(|page| page.items)
    }
    pub fn page(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        after: Option<&str>,
        limit: u16,
    ) -> Result<GitConnectionPage, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        if !(1..=100).contains(&limit) {
            return Err(PlatformError::Validation(
                "page limit must be between 1 and 100",
            ));
        }
        if let Some(cursor) = after
            && (cursor.len() != 36
                || !cursor.starts_with("gcn_")
                || !cursor[4..].bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(PlatformError::Validation("invalid connection cursor"));
        }
        let admin = ctx.require(Capability::InstallationManage, None).is_ok();
        let reader = ctx.require(Capability::ConnectionsRead, None).is_ok();
        if let Some(project) = project {
            ctx.require(Capability::RepositoriesWrite, Some(project))?;
            self.store
                .client
                .query_opt(
                    "SELECT id FROM projects WHERE id=$1 AND NOT archived",
                    &[&project],
                )
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::NotFound)?;
        } else if !admin && !reader {
            ctx.require(Capability::ConnectionsManage, None)?;
        }
        let rows = self.store.client.query(
            "SELECT to_jsonb(c)-'tokens' FROM git_connections c JOIN actors a ON a.id=c.owner_actor_id WHERE ($1::text IS NULL OR (c.status='active' AND a.active)) AND ($2 OR c.owner_actor_id=$3 OR ($1::text IS NOT NULL AND EXISTS(SELECT 1 FROM git_connection_projects g WHERE g.connection_id=c.id AND g.project_id=$1))) AND ($4::text IS NULL OR c.id>$4) ORDER BY c.id LIMIT $5",
            &[&project, &(admin || reader), &ctx.actor_id(), &after, &(i64::from(limit)+1)],
        ).map_err(PlatformStore::database_error)?;
        let mut items: Vec<GitConnection> = rows
            .iter()
            .map(|row| PlatformStore::decode(row.get(0)))
            .collect::<Result<_, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_cursor = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        Ok(GitConnectionPage { items, next_cursor })
    }
    pub fn revoke(&mut self, ctx: &AuthContext, id: &str) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "git.connection.revoke",
        )? {
            let _: () = PlatformStore::decode(value)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        let owner: String = tx
            .query_opt(
                "SELECT owner_actor_id FROM git_connections WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        Self::require_owner(&ctx, &owner)?;
        tx.execute("UPDATE git_connections SET status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND status='active'",&[&id]).map_err(PlatformStore::database_error)?;
        tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE connection_id=$1 AND authorization_status='authorized'",&[&id]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.connection_revoked",
            Some(id),
            json!({}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn require_owner(ctx: &AuthContext, owner: &str) -> Result<(), PlatformError> {
        if ctx.require(Capability::InstallationManage, None).is_ok() {
            return Ok(());
        }
        if ctx.actor_id() != owner {
            return Err(PlatformError::Forbidden);
        }
        ctx.require(Capability::ConnectionsManage, None)
    }
    pub fn share(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        project: &str,
    ) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "git.connection.share",
        )? {
            let _: () = PlatformStore::decode(value)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        tx.query_opt(
            "SELECT id FROM projects WHERE id=$1 AND NOT archived FOR SHARE",
            &[&project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let owner:String=tx.query_opt("SELECT owner_actor_id FROM git_connections WHERE id=$1 AND status='active' FOR SHARE",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?.get(0);
        Self::require_owner(&ctx, &owner)?;
        tx.execute("INSERT INTO git_connection_projects(connection_id,project_id,granted_by) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&id,&project,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.connection_project_granted",
            Some(id),
            json!({"project_id":project}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn unshare(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        project: &str,
    ) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "git.connection.unshare",
        )? {
            let _: () = PlatformStore::decode(value)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        // Use the same project -> connection lock order as final provider binding.
        tx.query_opt("SELECT id FROM projects WHERE id=$1 FOR SHARE", &[&project])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let owner: String = tx
            .query_opt(
                "SELECT owner_actor_id FROM git_connections WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        Self::require_owner(&ctx, &owner)?;
        let removed = tx
            .execute(
                "DELETE FROM git_connection_projects WHERE connection_id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?;
        let revoked = if removed > 0 {
            tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE connection_id=$1 AND project_id=$2 AND authorization_status='authorized'", &[&id,&project])
                .map_err(PlatformStore::database_error)?
        } else {
            0
        };
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.connection_project_revoked",
            Some(id),
            json!({"project_id":project,"grant_removed":removed > 0,"repositories_revoked":revoked}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    /// Global discovery is limited to the connection owner or installation administrator.
    /// Project delegation does not grant visibility into the entire remote account.
    pub fn can_discover(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<GitConnection, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        let row = self.store.client.query_opt(
            "SELECT to_jsonb(c)-'tokens',c.owner_actor_id FROM git_connections c JOIN actors a ON a.id=c.owner_actor_id WHERE c.id=$1 AND c.status='active' AND a.active",
            &[&id],
        ).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let owner: String = row.get(1);
        if ctx.require(Capability::ConnectionsRead, None).is_ok() {
            ctx.require(Capability::RepositoriesRead, None)?;
        } else {
            Self::require_owner(&ctx, &owner)?;
        }
        PlatformStore::decode(row.get(0))
    }
    pub fn can_use(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        project: &str,
    ) -> Result<GitConnection, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        let row=self.store.client.query_opt("SELECT to_jsonb(c)-'tokens',c.owner_actor_id FROM git_connections c JOIN actors a ON a.id=c.owner_actor_id WHERE c.id=$1 AND c.status='active' AND a.active",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let owner: String = row.get(1);
        let admin = ctx.require(Capability::InstallationManage, None).is_ok();
        if owner != ctx.actor_id() && !admin {
            let granted:bool=self.store.client.query_one("SELECT EXISTS(SELECT 1 FROM git_connection_projects WHERE connection_id=$1 AND project_id=$2)",&[&id,&project]).map_err(PlatformStore::database_error)?.get(0);
            if !granted {
                return Err(PlatformError::Forbidden);
            }
        }
        PlatformStore::decode(row.get(0))
    }
    pub fn record_access_failure(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(), PlatformError> {
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.provider_access_failed",
            Some(id),
            json!({"connection_revoked": false}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)
    }
}
