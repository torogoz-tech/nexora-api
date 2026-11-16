use crate::{PlatformStore, personal_mcp_key_store::PersonalMcpKeyStore};
use nexora_platform::{
    AuthContext, IssuedApiKey, PersonalMcpKeyCreate, PersonalMcpKeyListQuery, PlatformError,
};
use serde_json::Value;
impl PlatformStore {
    pub fn personal_mcp_keys(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        PersonalMcpKeyStore::list(self, ctx)
    }
    pub fn personal_mcp_key_page(
        &mut self,
        ctx: &AuthContext,
        query: &PersonalMcpKeyListQuery,
    ) -> Result<Value, PlatformError> {
        PersonalMcpKeyStore::list_page(self, ctx, Some(query))
    }
    pub fn issue_personal_mcp_key(
        &mut self,
        ctx: &AuthContext,
        input: &PersonalMcpKeyCreate,
    ) -> Result<IssuedApiKey, PlatformError> {
        PersonalMcpKeyStore::issue(self, ctx, input)
    }
    pub fn revoke_personal_mcp_key(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(), PlatformError> {
        PersonalMcpKeyStore::revoke(self, ctx, id)
    }
    pub fn record_mcp_activity(
        &mut self,
        ctx: &AuthContext,
        client: Option<(String, String)>,
    ) -> Result<(), PlatformError> {
        PersonalMcpKeyStore::activity(self, ctx, client)
    }
}
