use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    /// Public metadata only: required to render the login before authentication.
    pub fn public_installation_preferences(
        &mut self,
    ) -> Result<InstallationPreferences, PlatformError> {
        let value: Value = self
            .client
            .query_one(
                "SELECT jsonb_build_object('language', language, 'revision', revision)
                 FROM installation_preferences WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        Self::decode(value)
    }
    pub fn installation_preferences(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<InstallationPreferences, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let value: Value = tx
            .query_one(
                "SELECT jsonb_build_object('language', language, 'revision', revision)
                 FROM installation_preferences WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    pub fn change_installation_preferences(
        &mut self,
        ctx: &AuthContext,
        input: &InstallationPreferencesChange,
    ) -> Result<InstallationPreferences, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected preferences revision required",
            ));
        }
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "installation.preferences.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let before: Value = tx
            .query_one(
                "SELECT jsonb_build_object('language', language, 'revision', revision)
                 FROM installation_preferences WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if before["revision"].as_i64() != Some(input.expected_revision) {
            return Err(PlatformError::Conflict("installation preferences changed"));
        }
        let value: Value = tx
            .query_one(
                "UPDATE installation_preferences SET language=$1, revision=revision+1
                 WHERE singleton
                 RETURNING jsonb_build_object('language', language, 'revision', revision)",
                &[&input.language.as_str()],
            )
            .map_err(Self::database_error)?
            .get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "installation.preferences.changed",
            None,
            json!({"before":before,"after":value}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
}
