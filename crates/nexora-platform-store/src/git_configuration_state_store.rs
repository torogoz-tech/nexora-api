use crate::{
    GitNetworkPolicy, PlatformStore, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct GitConfigurationStateStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GitConfigurationStateStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn change(
        &mut self,
        ctx: &AuthContext,
        provider: GitProvider,
        input: &GitConfigurationStateChange,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected revision must be positive",
            ));
        }
        let (table, secret, api_column, origin, api_origin, operation) = match provider {
            GitProvider::Github => (
                "github_settings",
                "secrets",
                "api_origin",
                &policy.github_origin,
                &policy.github_api_origin,
                "github.configuration.state",
            ),
            GitProvider::Gitlab => (
                "gitlab_settings",
                "client_secret",
                "origin",
                &policy.gitlab_origin,
                &policy.gitlab_origin,
                "gitlab.configuration.state",
            ),
        };
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
            operation,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!(
                "{}:{}-settings",
                self.store.namespace,
                provider.as_str()
            )],
        )
        .map_err(PlatformStore::database_error)?;
        // All identifiers come from the exhaustive provider match, never request strings.
        let row = tx.query_opt(&format!("SELECT revision,enabled,origin,{api_column},redirect_uri FROM {table} WHERE singleton FOR UPDATE"), &[])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let revision: i64 = row.get(0);
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "provider configuration revision changed",
            ));
        }
        if input.enabled {
            policy.validate()?;
            if row.get::<_, String>(2) != *origin
                || row.get::<_, String>(3) != *api_origin
                || row.get::<_, String>(4)
                    != format!(
                        "{}/api/v1/git/{}/callback",
                        policy.callback_origin,
                        provider.as_str()
                    )
            {
                return Err(PlatformError::Conflict(
                    "provider operator origin changed; reconfigure credentials",
                ));
            }
        }
        let changed = row.get::<_, bool>(1) != input.enabled;
        let projection = format!("to_jsonb(s)-'{secret}'-'singleton'-'credential_revision'");
        let value: Value = if changed {
            tx.query_one(&format!("UPDATE {table} AS s SET enabled=$1,revision=revision+1 WHERE singleton RETURNING {projection}"), &[&input.enabled])
                .map_err(PlatformStore::database_error)?.get(0)
        } else {
            tx.query_one(
                &format!("SELECT {projection} FROM {table} s WHERE singleton"),
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .get(0)
        };
        if !input.enabled {
            let provider = provider.as_str();
            tx.execute("UPDATE git_connections SET status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE provider=$1 AND status='active'", &[&provider]).map_err(PlatformStore::database_error)?;
            tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE authorization_status='authorized' AND connection_id IN (SELECT id FROM git_connections WHERE provider=$1)", &[&provider]).map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.configuration_state_changed",
            Some(provider.as_str()),
            json!({"enabled":input.enabled,"revision":value["revision"],"changed":changed}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
}
