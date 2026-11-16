use crate::{
    PlatformStore, ProviderSecretEnvelope, ServiceRuntimeSettings, audit_writer::AuditWriter,
    installation_service_policy::InstallationServicePolicy, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn installation_service_settings(
        &mut self,
        ctx: &AuthContext,
        kind: InstallationService,
    ) -> Result<InstallationServiceSettings, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let value:Value=tx.query_one("SELECT to_jsonb(s)-'kind'-'secret'-'secret_revision'||jsonb_build_object('has_secret',secret IS NOT NULL) FROM installation_services s WHERE kind=$1",&[&kind.as_str()]).map_err(Self::database_error)?.get(0);
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    pub fn change_installation_service(
        &mut self,
        ctx: &AuthContext,
        kind: InstallationService,
        input: &InstallationServiceChange,
    ) -> Result<InstallationServiceSettings, PlatformError> {
        InstallationServicePolicy::validate(kind, &input.settings)?;
        if input.expected_revision < 1
            || input.remove_secret && input.secret.is_some()
            || input
                .secret
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 4096 || v.chars().any(char::is_control))
            || kind == InstallationService::Openai
                && input
                    .secret
                    .as_ref()
                    .is_some_and(|v| !v.bytes().all(|b| b.is_ascii_graphic()))
        {
            return Err(PlatformError::Validation("invalid service credential"));
        }
        let operation = "installation.service.change";
        let receipt = self.request_context.clone();
        self.fresh(ctx)?
            .require(Capability::InstallationManage, None)?;
        let cipher = self.provider_secret_cipher()?;
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
        let row=tx.query_one("SELECT settings,secret,secret_revision,revision FROM installation_services WHERE kind=$1 FOR UPDATE",&[&kind.as_str()]).map_err(Self::database_error)?;
        if row.get::<_, i64>(3) != input.expected_revision {
            return Err(PlatformError::Conflict("service settings changed"));
        }
        let previous: Value = row.get(0);
        let mut secret: Option<Value> = row.get(1);
        let mut secret_revision: i64 = row.get(2);
        if secret.is_some()
            && InstallationServicePolicy::binding(kind, &previous)
                != InstallationServicePolicy::binding(kind, &input.settings)
            && input.secret.is_none()
            && !input.remove_secret
        {
            return Err(PlatformError::Validation(
                "new credential required when changing service destination",
            ));
        }
        if input.remove_secret {
            secret = None;
        }
        if let Some(value) = &input.secret {
            secret_revision += 1;
            secret = Some(
                serde_json::to_value(cipher.seal(
                    &InstallationServicePolicy::context(kind, &input.settings, secret_revision),
                    value.as_bytes(),
                )?)
                .map_err(|_| PlatformError::Storage)?,
            );
        }
        let enabled = input.settings["enabled"].as_bool() == Some(true);
        if enabled
            && (kind == InstallationService::Openai
                || !input.settings["username"].as_str().unwrap_or("").is_empty())
            && secret.is_none()
        {
            return Err(PlatformError::Validation(
                "service credential required before enabling",
            ));
        }
        let value:Value=tx.query_one("UPDATE installation_services s SET settings=$2,secret=$3,secret_revision=$4,managed=true,revision=revision+1 WHERE kind=$1 RETURNING to_jsonb(s)-'kind'-'secret'-'secret_revision'||jsonb_build_object('has_secret',secret IS NOT NULL)",&[&kind.as_str(),&input.settings,&secret,&secret_revision]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            Some(kind.as_str()),
            json!({"revision":value["revision"],"credential_replaced":input.secret.is_some(),"credential_removed":input.remove_secret}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    /// Internal host runtime use only, after the caller authorizes the operation.
    pub fn service_runtime_settings(
        &mut self,
        kind: InstallationService,
    ) -> Result<Option<ServiceRuntimeSettings>, PlatformError> {
        self.service_credentials(kind, false)
    }
    pub fn smtp_runtime_settings(
        &mut self,
    ) -> Result<Option<ServiceRuntimeSettings>, PlatformError> {
        self.service_credentials(InstallationService::Smtp, true)
    }
    fn service_credentials(
        &mut self,
        kind: InstallationService,
        allow_disabled: bool,
    ) -> Result<Option<ServiceRuntimeSettings>, PlatformError> {
        let row=self.client.query_one("SELECT settings,secret,secret_revision,managed FROM installation_services WHERE kind=$1",&[&kind.as_str()]).map_err(Self::database_error)?;
        if !row.get::<_, bool>(3) {
            return Ok(None);
        }
        let settings: Value = row.get(0);
        InstallationServicePolicy::validate(kind, &settings)?;
        if !allow_disabled && settings["enabled"].as_bool() != Some(true) {
            return Err(PlatformError::Unavailable);
        }
        let encrypted: Option<Value> = row.get(1);
        let secret = encrypted
            .map(|v| {
                let envelope: ProviderSecretEnvelope = Self::decode(v)?;
                self.provider_secret_cipher()?.open(
                    &InstallationServicePolicy::context(kind, &settings, row.get(2)),
                    &envelope,
                )
            })
            .transpose()?;
        Ok(Some(ServiceRuntimeSettings { settings, secret }))
    }
}
