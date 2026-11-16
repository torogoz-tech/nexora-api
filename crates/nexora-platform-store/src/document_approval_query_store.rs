use crate::{PlatformStore, document_policy_store::DocumentPolicyStore};
use nexora_platform::*;
use serde_json::Value;
pub(crate) struct DocumentApprovalQueryStore;
impl DocumentApprovalQueryStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        version: &str,
        id: &str,
    ) -> Result<DocumentApproval, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        let value:Value=tx.query_opt("SELECT to_jsonb(a) FROM document_approvals a WHERE id=$1 AND project_id=$2 AND document_id=$3 AND version_id=$4",&[&id,&project,&document,&version]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
