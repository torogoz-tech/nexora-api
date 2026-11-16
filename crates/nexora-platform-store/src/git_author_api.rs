use crate::PlatformStore;
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn git_authors(
        &mut self,
        ctx: &AuthContext,
        q: &GitAuthorQuery,
    ) -> Result<Value, PlatformError> {
        crate::git_author_query_store::GitAuthorQueryStore::authors(self, ctx, q)
    }
    pub fn git_author_aliases(
        &mut self,
        ctx: &AuthContext,
        q: &GitAuthorQuery,
    ) -> Result<Value, PlatformError> {
        crate::git_author_query_store::GitAuthorQueryStore::aliases(self, ctx, q)
    }
    pub fn git_author_options(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<Value, PlatformError> {
        crate::git_author_query_store::GitAuthorQueryStore::options(self, ctx, project)
    }
    pub fn create_git_author_alias(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &GitAuthorAliasCreate,
    ) -> Result<Value, PlatformError> {
        crate::git_author_alias_store::GitAuthorAliasStore::create(self, ctx, actor, input)
    }
    pub fn revoke_git_author_alias(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        id: &str,
        input: &GitAuthorAliasRevoke,
    ) -> Result<Value, PlatformError> {
        crate::git_author_alias_store::GitAuthorAliasStore::revoke(self, ctx, actor, id, input)
    }
}
