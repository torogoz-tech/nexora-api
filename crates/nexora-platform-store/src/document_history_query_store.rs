use crate::{
    PlatformStore, document_page_validation::DocumentPageValidation,
    document_policy_store::DocumentPolicyStore,
};
use nexora_platform::*;
use serde_json::Value;

pub(crate) struct DocumentHistoryQueryStore;
impl DocumentHistoryQueryStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<i64>,
        limit: u16,
    ) -> Result<DocumentHistoryPage, PlatformError> {
        DocumentPageValidation::limit(limit)?;
        if after.is_some_and(|value| value < 0) {
            return Err(PlatformError::Validation("invalid document history cursor"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        DocumentPolicyStore::definition(&mut tx, project, id, false)?;
        let rows=tx.query("SELECT to_jsonb(h) FROM document_history h WHERE project_id=$1 AND document_id=$2 AND ($3::bigint IS NULL OR h.id>$3) ORDER BY h.id LIMIT $4",&[&project,&id,&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
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
            items.push(PlatformStore::decode::<DocumentHistoryEvent>(value)?);
        }
        let next_after = if more {
            items.last().map(|item| item.id)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DocumentHistoryPage { items, next_after })
    }
}
