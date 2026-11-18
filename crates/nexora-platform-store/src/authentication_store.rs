use crate::{PlatformStore, SecretGenerator};
use nexora_platform::*;
use postgres::types::Type;
use serde_json::Value;
use subtle::ConstantTimeEq;
pub(crate) struct AuthenticationStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> AuthenticationStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn authenticate(&mut self, token: &str) -> Result<AuthContext, PlatformError> {
        if token.len() > 256 {
            return Err(PlatformError::Unauthorized);
        }
        let (method, id) = if let Some(rest) = token.strip_prefix("nxk_") {
            (
                AuthMethod::ApiKey,
                rest.split_once('.').ok_or(PlatformError::Unauthorized)?.0,
            )
        } else if let Some(rest) = token.strip_prefix("nxs_") {
            (
                AuthMethod::Session,
                rest.split_once('.').ok_or(PlatformError::Unauthorized)?.0,
            )
        } else {
            return Err(PlatformError::Unauthorized);
        };
        if id.is_empty() || id.len() > 128 {
            return Err(PlatformError::Unauthorized);
        }
        let digest = SecretGenerator::digest(token);
        let table = if method == AuthMethod::ApiKey {
            "api_keys"
        } else {
            "sessions"
        };
        let expiry = if method == AuthMethod::ApiKey {
            "(expires_at IS NULL OR expires_at>now())"
        } else {
            "expires_at>now() AND absolute_expires_at>now()"
        };
        let row = self
            .store
            .client
            .query_typed_opt(
                &format!(
                    "SELECT digest FROM {table} WHERE id=$1 AND revoked_at IS NULL AND {expiry}"
                ),
                &[(&id, Type::TEXT)],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Unauthorized)?;
        let stored: Vec<u8> = row.get(0);
        if !bool::from(stored.ct_eq(&digest)) {
            return Err(PlatformError::Unauthorized);
        }
        if method == AuthMethod::Session {
            self.store.client.execute_typed(
                "UPDATE sessions SET expires_at=LEAST(clock_timestamp()+interval '30 minutes',absolute_expires_at),last_activity_at=clock_timestamp() WHERE id=$1 AND digest=$2 AND access_stage='ready' AND revoked_at IS NULL AND expires_at>clock_timestamp() AND absolute_expires_at>clock_timestamp() AND last_activity_at<clock_timestamp()-interval '60 seconds'",
                &[(&id, Type::TEXT), (&digest, Type::BYTEA)],
            ).map_err(PlatformStore::database_error)?;
        }
        let context = self.load(method, id)?;
        if method == AuthMethod::ApiKey {
            self.store.client.execute_typed("UPDATE api_keys SET last_used_at=now() WHERE id=$1 AND (last_used_at IS NULL OR last_used_at<now()-interval '60 seconds')",&[(&id, Type::TEXT)]).map_err(PlatformStore::database_error)?;
        }
        Ok(context)
    }
    pub fn refresh(&mut self, ctx: &AuthContext) -> Result<AuthContext, PlatformError> {
        let current = self.load(ctx.method(), ctx.credential_id())?;
        if current.actor_id() != ctx.actor_id() {
            return Err(PlatformError::Unauthorized);
        }
        Ok(current)
    }
    pub(crate) fn load(
        &mut self,
        method: AuthMethod,
        id: &str,
    ) -> Result<AuthContext, PlatformError> {
        let query = if method == AuthMethod::ApiKey {
            "SELECT a.id,a.kind,k.scope FROM api_keys k JOIN actors a ON a.id=k.actor_id WHERE k.id=$1 AND a.active AND k.revoked_at IS NULL AND (k.expires_at IS NULL OR k.expires_at>now())"
        } else {
            "SELECT a.id,a.kind,NULL::jsonb FROM sessions s JOIN actors a ON a.id=s.actor_id JOIN users u ON u.actor_id=a.id WHERE s.access_stage='ready' AND s.id=$1 AND a.active AND a.kind='human' AND s.revoked_at IS NULL AND s.expires_at>now() AND s.absolute_expires_at>now()"
        };
        let row = self
            .store
            .client
            .query_typed_opt(query, &[(&id, Type::TEXT)])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Unauthorized)?;
        let actor: String = row.get(0);
        let kind: ActorKind = PlatformStore::decode(Value::String(row.get(1)))?;
        let scope = row
            .get::<_, Option<Value>>(2)
            .map(PlatformStore::decode::<KeyScope>)
            .transpose()?;
        if let Some(scope) = &scope {
            scope.validate()?;
        }
        let grants = self
            .store
            .client
            .query_typed(
                "SELECT capability,project_id FROM actor_grants WHERE actor_id=$1",
                &[(&actor, Type::TEXT)],
            )
            .map_err(PlatformStore::database_error)?
            .iter()
            .map(|r| {
                Ok(CapabilityGrant::new(
                    PlatformStore::decode(Value::String(r.get(0)))?,
                    r.get(1),
                ))
            })
            .collect::<Result<Vec<_>, PlatformError>>()?;
        Ok(AuthContext::new(
            actor,
            kind,
            method,
            id.into(),
            grants,
            scope,
        ))
    }
}
