use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn effective_permissions(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        // Even an actor without grants must have a currently live credential.
        match PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, None) {
            Ok(()) | Err(PlatformError::Forbidden) => {}
            Err(error) => return Err(error),
        }
        let role: Option<String> = tx
            .query_opt(
                "SELECT role FROM actor_roles WHERE actor_id=$1 FOR SHARE",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .map(|row| row.get(0));
        let rows = tx.query("SELECT capability,project_id FROM actor_grants WHERE actor_id=$1 ORDER BY capability,project_id", &[&ctx.actor_id()]).map_err(Self::database_error)?;
        let mut grants = Vec::new();
        for row in rows {
            let capability: Capability = serde_json::from_value(json!(row.get::<_, String>(0)))
                .map_err(|_| PlatformError::Storage)?;
            let project: Option<String> = row.get(1);
            let scopes = if project.is_none()
                && ctx
                    .scope()
                    .is_some_and(|scope| scope.all_projects && !scope.installation)
            {
                tx.query(
                    "SELECT id FROM projects WHERE NOT archived ORDER BY id",
                    &[],
                )
                .map_err(Self::database_error)?
                .iter()
                .map(|row| Some(row.get::<_, String>(0)))
                .collect::<Vec<_>>()
            } else if project.is_none()
                && ctx
                    .scope()
                    .is_some_and(|scope| !scope.all_projects && !scope.project_ids.is_empty())
            {
                let mut scopes = ctx
                    .scope()
                    .unwrap()
                    .project_ids
                    .iter()
                    .cloned()
                    .map(Some)
                    .collect::<Vec<_>>();
                if ctx.scope().unwrap().installation {
                    scopes.push(None);
                }
                scopes
            } else {
                vec![project]
            };
            for scope in scopes {
                match PlatformAuthority::require(&mut tx, ctx, capability, scope.as_deref()) {
                    Ok(()) => {
                        if !grants
                            .iter()
                            .any(|v| v == &json!({"capability":capability,"project_id":scope}))
                        {
                            grants.push(json!({"capability":capability,"project_id":scope}));
                        }
                    }
                    Err(PlatformError::Forbidden) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"actor_id":ctx.actor_id(),"role":role,"grants":grants}))
    }
}
