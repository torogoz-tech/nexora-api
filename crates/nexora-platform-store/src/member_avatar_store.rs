use crate::{
    PlatformStore, audit_writer::AuditWriter, member_avatar_authority::MemberAvatarAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn member_avatar(
        &mut self,
        ctx: &AuthContext,
        member: &str,
    ) -> Result<Vec<u8>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        MemberAvatarAuthority::read(&mut tx, ctx, member)?;
        let bytes = tx
            .query_opt(
                "SELECT image_png FROM user_avatars WHERE actor_id=$1 AND image_png IS NOT NULL",
                &[&member],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(bytes)
    }
    pub fn authorize_member_avatar_change(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<(), PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        MemberAvatarAuthority::change(&mut tx, ctx)?;
        tx.commit().map_err(Self::database_error)
    }
    pub fn change_member_avatar(
        &mut self,
        ctx: &AuthContext,
        image: Option<Vec<u8>>,
    ) -> Result<Value, PlatformError> {
        if image
            .as_ref()
            .is_some_and(|bytes| bytes.len() > 2097152 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        {
            return Err(PlatformError::Validation("invalid normalized avatar"));
        }
        let operation = if image.is_some() {
            "member.avatar.change"
        } else {
            "member.avatar.delete"
        };
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        MemberAvatarAuthority::change(&mut tx, ctx)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let revision:i64=tx.query_one("INSERT INTO user_avatars(actor_id,revision,image_png) VALUES($1,1,$2) ON CONFLICT(actor_id) DO UPDATE SET revision=user_avatars.revision+1,image_png=EXCLUDED.image_png RETURNING revision", &[&ctx.actor_id(),&image]).map_err(Self::database_error)?.get(0);
        let value = json!({"avatar_revision":revision});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            None,
            value.clone(),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
