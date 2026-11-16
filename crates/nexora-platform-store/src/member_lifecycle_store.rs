use crate::{PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority};
use nexora_platform::{AuthContext, AuthMethod, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn change_member_archived(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        archived: bool,
        revision: i64,
    ) -> Result<Value, PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.actor_id() == actor || revision < 0 {
            return Err(PlatformError::Forbidden);
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let row=tx.query_opt("SELECT a.active,u.archived_at IS NOT NULL,u.archived_was_active,u.lifecycle_revision,EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=a.id AND r.role='super_admin') FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.id=$1 AND a.kind='human' FOR UPDATE OF a,u",&[&actor]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, bool>(4) {
            return Err(PlatformError::Forbidden);
        }
        if row.get::<_, i64>(3) != revision {
            return Err(PlatformError::Conflict("member lifecycle changed"));
        }
        if row.get::<_, bool>(1) != archived {
            let active = if archived {
                false
            } else {
                row.get::<_, bool>(2)
            };
            tx.execute("UPDATE users SET archived_at=CASE WHEN $2 THEN clock_timestamp() ELSE NULL END,archived_was_active=CASE WHEN $2 THEN $3 ELSE archived_was_active END,lifecycle_revision=lifecycle_revision+1 WHERE actor_id=$1",&[&actor,&archived,&row.get::<_,bool>(0)]).map_err(Self::database_error)?;
            tx.execute(
                "UPDATE actors SET active=$2 WHERE id=$1",
                &[&actor, &active],
            )
            .map_err(Self::database_error)?;
            tx.execute("UPDATE sessions SET revoked_at=clock_timestamp() WHERE actor_id=$1 AND revoked_at IS NULL",&[&actor]).map_err(Self::database_error)?;
            if archived {
                tx.execute("UPDATE api_keys SET revoked_at=clock_timestamp() WHERE actor_id=$1 AND revoked_at IS NULL",&[&actor]).map_err(Self::database_error)?;
                tx.execute("UPDATE user_email_changes SET used_at=clock_timestamp(),token_secret=NULL WHERE actor_id=$1 AND used_at IS NULL",&[&actor]).map_err(Self::database_error)?;
            }
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                if archived {
                    "member.archived"
                } else {
                    "member.restored"
                },
                Some(actor),
                json!({"revision":revision+1}),
            )?;
        }
        let row=tx.query_one("SELECT jsonb_build_object('archived',archived_at IS NOT NULL,'lifecycle_revision',lifecycle_revision) FROM users WHERE actor_id=$1",&[&actor]).map_err(Self::database_error)?;
        let value = row.get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
