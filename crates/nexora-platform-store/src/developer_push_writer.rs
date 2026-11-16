use crate::PlatformStore;
use nexora_platform::PlatformError;
use postgres::Transaction;
use serde_json::Value;

/// Invoked exclusively after provider signature, delivery and repository checks.
pub(crate) struct DeveloperPushWriter;
impl DeveloperPushWriter {
    pub fn record(
        tx: &mut Transaction<'_>,
        provider: &str,
        host: &str,
        scope: &str,
        delivery: &str,
        repository: &str,
        payload: &Value,
    ) -> Result<(), PlatformError> {
        // Stable provider authenticated sender, never author names/emails or claimed commit authors.
        let remote = if provider == "github" {
            payload["sender"]["id"].as_u64()
        } else {
            payload["user_id"].as_u64()
        };
        let Some(remote) = remote.filter(|id| *id > 0).map(|id| id.to_string()) else {
            return Ok(());
        };
        if payload["before"]
            .as_str()
            .is_some_and(|before| Some(before) == payload["after"].as_str())
        {
            return Ok(());
        }
        // A branch deletion is not code activity.
        if payload["after"]
            .as_str()
            .is_none_or(|sha| sha.is_empty() || sha.chars().all(|c| c == '0'))
        {
            return Ok(());
        }
        tx.execute("INSERT INTO developer_push_activity(actor_id,provider,scope_id,delivery_id,project_id) SELECT DISTINCT c.owner_actor_id,$1,$3,$4,r.project_id FROM git_connections c JOIN actors a ON a.id=c.owner_actor_id JOIN actor_roles ar ON ar.actor_id=a.id JOIN project_repositories r ON r.id=$5 CROSS JOIN labor_calendar_settings s WHERE c.provider=$1 AND c.host=$2 AND c.remote_account_id=$6 AND c.status='active' AND (SELECT count(DISTINCT identity.owner_actor_id) FROM git_connections identity JOIN actors linked ON linked.id=identity.owner_actor_id AND linked.active WHERE identity.provider=$1 AND identity.host=$2 AND identity.remote_account_id=$6 AND identity.status='active')=1 AND a.active AND a.kind='human' AND ar.role IN ('developer','technical_lead') AND NOT r.archived AND r.authorization_status='authorized' AND (ar.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=a.id AND mp.project_id=r.project_id)) ON CONFLICT DO NOTHING",&[&provider,&host,&scope,&delivery,&repository,&remote]).map_err(PlatformStore::database_error)?;
        Ok(())
    }
}
