use crate::{GitNetworkPolicy, PlatformStore, authentication_store::AuthenticationStore};
use nexora_platform::*;
use serde_json::json;
impl PlatformStore {
    /// Worker continuation uses the original verified credential's live authority, never an ambient service identity.
    pub fn resume_one_task_transition(
        &mut self,
        policy: &GitNetworkPolicy,
    ) -> Result<bool, PlatformError> {
        let row=self.client.query_opt("SELECT id,task_id,actor_id,credential_id,auth_method FROM task_transition_runs WHERE (record->>'status'='ready' OR (record->>'status'='waiting_external' AND updated_at<clock_timestamp()-interval '30 seconds')) ORDER BY updated_at,id LIMIT 1",&[]).map_err(Self::database_error)?;
        let Some(row) = row else { return Ok(false) };
        let id: String = row.get(0);
        let task: String = row.get(1);
        let actor: String = row.get(2);
        let credential: String = row.get(3);
        let method: String = row.get(4);
        let context = AuthenticationStore::new(self).load(
            if method == "api_key" {
                AuthMethod::ApiKey
            } else {
                AuthMethod::Session
            },
            &credential,
        );
        let ctx = match context {
            Ok(ctx) if ctx.actor_id() == actor => ctx,
            _ => {
                self.client.execute("UPDATE task_transition_runs SET record=record||$2::jsonb,updated_at=clock_timestamp() WHERE id=$1 AND record->>'status' IN ('ready','waiting_external')",&[&id,&json!({"status":"failed","error_code":"transition_authority_changed","message":"Original execution credential expired or was revoked. Restore authority and retry explicitly."})]).map_err(Self::database_error)?;
                return Ok(true);
            }
        };
        match self.retry_task_transition(&ctx, &task, &id, policy) {
            Ok(_) => Ok(true),
            Err(PlatformError::Storage) => Err(PlatformError::Storage),
            Err(error) => {
                // Live scope revocation may fail before engine run creation/checkpoint. Avoid an endless ready queue.
                if matches!(
                    error,
                    PlatformError::Forbidden
                        | PlatformError::Unauthorized
                        | PlatformError::NotFound
                ) {
                    self.client.execute("UPDATE task_transition_runs SET record=record||$2::jsonb,updated_at=clock_timestamp() WHERE id=$1 AND record->>'status' IN ('ready','waiting_external')",&[&id,&json!({"status":"failed","error_code":"transition_authority_changed","message":"Original execution authority changed. Reload the task and retry explicitly."})]).map_err(Self::database_error)?;
                }
                Ok(true)
            }
        }
    }
}
