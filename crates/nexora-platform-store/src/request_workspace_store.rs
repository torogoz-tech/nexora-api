use crate::PlatformStore;
use nexora_platform::*;
impl PlatformStore {
    /// Enforce the workspace boundary even for stale sessions or legacy technical grants.
    pub fn authorize_request_workspace(
        &mut self,
        ctx: &AuthContext,
        path: &str,
    ) -> Result<(), PlatformError> {
        let external = self.client.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('user','supervisor')",&[&ctx.actor_id()]).map_err(Self::database_error)?.is_some();
        if !external {
            return Ok(());
        }
        if path == "/mcp" {
            return Ok(());
        }
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        if path.starts_with("/api/v1/me/mcp-keys")
            || path == "/api/v1/requests"
            || path.starts_with("/api/v1/requests/")
            || matches!(
                path,
                "/api/v1/me/permissions" | "/api/v1/auth/logout" | "/api/v1/auth/session"
            )
        {
            Ok(())
        } else {
            Err(PlatformError::Forbidden)
        }
    }
    pub fn authorize_request_tool(
        &mut self,
        ctx: &AuthContext,
        name: &str,
    ) -> Result<(), PlatformError> {
        let external=self.client.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('user','supervisor')",&[&ctx.actor_id()]).map_err(Self::database_error)?.is_some();
        if external
            && !matches!(
                name,
                "nexora_request_report"
                    | "nexora_request_list"
                    | "nexora_request_get"
                    | "nexora_request_types_list"
                    | "nexora_request_products_list"
                    | "nexora_request_create"
                    | "nexora_request_comment"
                    | "nexora_request_attachment_upload"
                    | "nexora_request_attachment_get"
                    | "nexora_permissions_get"
            )
        {
            return Err(PlatformError::Forbidden);
        }
        Ok(())
    }
}
