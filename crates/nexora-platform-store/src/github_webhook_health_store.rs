use crate::{
    GitNetworkPolicy, PlatformStore, github_client::GithubClient,
    github_settings_store::GithubSettingsStore, platform_authority::PlatformAuthority,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn github_webhook_health(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let row=tx.query_opt("SELECT h.status||jsonb_build_object('checked_at',h.checked_at,'current',h.configuration_revision=s.revision) FROM github_webhook_health h CROSS JOIN github_settings s WHERE h.singleton AND s.singleton",&[]).map_err(Self::database_error)?;
        let status = row
            .map(|row| row.get::<_, Value>(0))
            .unwrap_or(json!({"current":false}));
        let last: Option<String> = tx
            .query_one(
                "SELECT max(received_at)::text FROM git_webhook_deliveries WHERE provider='github'",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"status":status,"last_delivery_at":last}))
    }
    pub fn check_github_webhook(
        &mut self,
        ctx: &AuthContext,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        self.reconcile_github_webhook(policy)
    }
    pub(crate) fn reconcile_github_webhook(
        &mut self,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let (config, secrets) = GithubSettingsStore::new(self).load(policy)?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("{}:github-settings", self.namespace)],
        )
        .map_err(Self::database_error)?;
        let revision: i64 = tx
            .query_one(
                "SELECT revision FROM github_settings WHERE singleton AND enabled FOR SHARE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if config.revision != revision {
            return Err(PlatformError::Conflict(
                "GitHub configuration revision changed",
            ));
        }
        let url = format!("{}/api/v1/git/github/webhook", policy.callback_origin);
        let result = GithubClient::new(policy).configure_webhook(
            &config.client_id,
            &secrets.private_key_pem,
            &url,
            &secrets.webhook_secret,
        );
        let status = match &result {
            Ok(value) => value.clone(),
            Err(_) => json!({"url":url,"configuration_verified":false}),
        };
        tx.execute("INSERT INTO github_webhook_health(singleton,configuration_revision,status) VALUES(true,$1,$2) ON CONFLICT(singleton) DO UPDATE SET configuration_revision=$1,status=$2,checked_at=clock_timestamp()",&[&revision,&status]).map_err(Self::database_error)?;
        tx.commit().map_err(Self::database_error)?;
        result
    }
}
