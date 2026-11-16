use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn change_member_position(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &MemberPositionChange,
    ) -> Result<Value, PlatformError> {
        if input.expected_revision < 0
            || input.position.len() > 160
            || input.position.chars().any(char::is_control)
        {
            return Err(PlatformError::Validation("invalid member position"));
        }
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND kind='human' FOR UPDATE",
            &[&actor],
        )
        .map_err(Self::database_error)?
        .ok_or(PlatformError::NotFound)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "member.position.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let revision: i64 = tx
            .query_opt(
                "SELECT revision FROM member_positions WHERE actor_id=$1",
                &[&actor],
            )
            .map_err(Self::database_error)?
            .map_or(0, |row| row.get(0));
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict("member position changed"));
        }
        let value: Value = tx.query_one("INSERT INTO member_positions(actor_id,position,revision) VALUES($1,$2,1) ON CONFLICT(actor_id) DO UPDATE SET position=EXCLUDED.position,revision=member_positions.revision+1 RETURNING jsonb_build_object('position',position,'position_revision',revision)", &[&actor,&input.position.trim()]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "member.position.change",
            Some(actor),
            value.clone(),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
