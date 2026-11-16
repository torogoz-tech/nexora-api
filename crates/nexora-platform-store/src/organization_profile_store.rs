use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn organization_profile(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<OrganizationProfile, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let value:Value=tx.query_one("SELECT to_jsonb(p)-'singleton'-'logo'||jsonb_build_object('has_logo',logo IS NOT NULL) FROM organization_profile p WHERE singleton",&[]).map_err(Self::database_error)?.get(0);
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    pub fn public_organization_branding(&mut self) -> Result<OrganizationBranding, PlatformError> {
        let value:Value=self.client.query_one("SELECT jsonb_build_object('name',name,'has_logo',logo IS NOT NULL,'revision',revision) FROM organization_profile WHERE singleton",&[]).map_err(Self::database_error)?.get(0);
        Self::decode(value)
    }
    pub fn organization_logo(&mut self) -> Result<Vec<u8>, PlatformError> {
        self.client
            .query_opt(
                "SELECT logo FROM organization_profile WHERE singleton AND logo IS NOT NULL",
                &[],
            )
            .map_err(Self::database_error)?
            .map(|r| r.get(0))
            .ok_or(PlatformError::NotFound)
    }
    pub fn change_organization_profile(
        &mut self,
        ctx: &AuthContext,
        input: &OrganizationProfileChange,
        logo: Option<Option<Vec<u8>>>,
    ) -> Result<OrganizationProfile, PlatformError> {
        if input.expected_revision < 1
            || input.name.as_ref().is_some_and(|s| {
                s.trim().is_empty() || s.len() > 160 || s.chars().any(char::is_control)
            })
            || input.address.as_ref().is_some_and(|s| {
                s.len() > 2000
                    || s.chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            })
            || input
                .phone
                .as_ref()
                .is_some_and(|s| s.len() > 64 || s.chars().any(char::is_control))
            || input.country_code.as_ref().is_some_and(|s| {
                !s.is_empty() && (s.len() != 2 || !s.bytes().all(|c| c.is_ascii_uppercase()))
            })
            || logo
                .as_ref()
                .and_then(|s| s.as_ref())
                .is_some_and(|v| v.len() > 2097152)
        {
            return Err(PlatformError::Validation("invalid organization profile"));
        }
        let operation = if logo.is_some() {
            "organization.logo.change"
        } else {
            "organization.profile.change"
        };
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let row = tx
            .query_one(
                "SELECT revision FROM organization_profile WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(Self::database_error)?;
        if row.get::<_, i64>(0) != input.expected_revision {
            return Err(PlatformError::Conflict("organization profile changed"));
        }
        let replace_logo = logo.is_some();
        let bytes = logo.flatten();
        let value:Value=tx.query_one("UPDATE organization_profile p SET name=COALESCE($1,name),address=COALESCE($2,address),phone=COALESCE($3,phone),country_code=COALESCE($4,country_code),logo=CASE WHEN $5 THEN $6 ELSE logo END,revision=revision+1 WHERE singleton RETURNING to_jsonb(p)-'singleton'-'logo'||jsonb_build_object('has_logo',logo IS NOT NULL)",&[&input.name.as_ref().map(|v|v.trim()),&input.address,&input.phone,&input.country_code,&replace_logo,&bytes]).map_err(Self::database_error)?.get(0);
        if let Some(name) = &input.name {
            tx.execute(
                "UPDATE installation SET organization_name=$1 WHERE singleton",
                &[&name.trim()],
            )
            .map_err(Self::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            None,
            json!({"revision":value["revision"]}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
}
