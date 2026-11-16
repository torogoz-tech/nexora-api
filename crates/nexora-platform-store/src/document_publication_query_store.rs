use crate::{
    PlatformStore, document_page_validation::DocumentPageValidation,
    document_policy_store::DocumentPolicyStore,
};
use nexora_platform::*;
use serde_json::Value;

pub(crate) struct DocumentPublicationQueryStore;
impl DocumentPublicationQueryStore {
    // Live ID traversal, not a claim of insertion-time ordering or a snapshot export.
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<&str>,
        limit: u16,
    ) -> Result<DocumentPublicationPage, PlatformError> {
        DocumentPageValidation::limit(limit)?;
        DocumentPageValidation::cursor(after)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        DocumentPolicyStore::definition(&mut tx, project, id, false)?;
        let rows=tx.query("SELECT to_jsonb(p) FROM document_publications p WHERE project_id=$1 AND document_id=$2 AND ($3::text IS NULL OR p.id>$3) ORDER BY p.id LIMIT $4",&[&project,&id,&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = Vec::new();
        let mut bytes = 0usize;
        let mut more = false;
        for row in rows {
            let value: Value = row.get(0);
            let cost = serde_json::to_vec(&value)
                .map_err(|_| PlatformError::Storage)?
                .len();
            if items.len() >= usize::from(limit) || (!items.is_empty() && bytes + cost > 262144) {
                more = true;
                break;
            }
            bytes += cost;
            items.push(PlatformStore::decode::<DocumentPublication>(value)?);
        }
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DocumentPublicationPage { items, next_after })
    }
}
