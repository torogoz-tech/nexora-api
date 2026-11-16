use crate::{PlatformStore, repository_snapshot_scope::RepositorySnapshotScope};
use nexora_platform::{AuthContext, PlatformError};
use nexora_runtime::{DatabaseRuntime, KnowledgeQuery};
use serde_json::{Value, json};

pub(crate) struct RepositoryCodeStore;
impl RepositoryCodeStore {
    pub fn query(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        query: &KnowledgeQuery,
    ) -> Result<Value, PlatformError> {
        query
            .validate()
            .map_err(|_| PlatformError::Validation("invalid code query"))?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let scope = RepositorySnapshotScope::lock(&mut tx, ctx, project, repository, snapshot)?;
        if store.code_runtime.is_none() {
            store.code_runtime = Some(
                DatabaseRuntime::connect(&store.connections).map_err(|_| PlatformError::Storage)?,
            );
        }
        let runtime = store.code_runtime.as_mut().ok_or(PlatformError::Storage)?;
        let header = scope.verify(runtime)?;
        let tenant = header.tenant_id;
        let repository_id = header.repository_id;
        let snapshot_id = header.id;
        let result = runtime.query(&tenant, &repository_id, Some(&snapshot_id), query);
        let mut result = match result {
            Ok(value) => value,
            Err(_) => {
                store.code_runtime = None;
                return Err(PlatformError::Storage);
            }
        };
        result["project_id"] = json!(project);
        result["repository_id"] = json!(repository);
        result["commit"] = json!(scope.commit);
        if let Some(origin) = &scope.local_origin {
            result["local_origin"] = json!(origin);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
