use crate::{
    PlatformStore, approval_request_authority::ApprovalRequestAuthority,
    document_page_validation::DocumentPageValidation,
};
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct ApprovalRequestQueryStore;
impl ApprovalRequestQueryStore {
    pub fn row(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
        lock: bool,
    ) -> Result<ApprovalRequest, PlatformError> {
        let sql = if lock {
            "SELECT to_jsonb(r)||jsonb_build_object('approval_id',COALESCE(task_approval_id,document_approval_id)) FROM approval_requests r WHERE id=$1 AND project_id=$2 FOR UPDATE"
        } else {
            "SELECT to_jsonb(r)||jsonb_build_object('approval_id',COALESCE(task_approval_id,document_approval_id)) FROM approval_requests r WHERE id=$1 AND project_id=$2"
        };
        let value = tx
            .query_opt(sql, &[&id, &project])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        PlatformStore::decode(value)
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ApprovalRequest, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let access = ApprovalRequestAuthority::readable(&mut tx, ctx, project)?;
        let result = Self::row(&mut tx, project, id, false)?;
        if (result.target_kind == "task" && !access.0)
            || (result.target_kind == "document_version" && !access.1)
        {
            return Err(PlatformError::NotFound);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        status: Option<ApprovalRequestStatus>,
    ) -> Result<ApprovalRequestPage, PlatformError> {
        DocumentPageValidation::limit(limit)?;
        DocumentPageValidation::cursor(after)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let access = ApprovalRequestAuthority::readable(&mut tx, ctx, project)?;
        let status = status.map(ApprovalRequestStatus::as_str);
        let rows=tx.query("SELECT jsonb_build_object('id',id,'project_id',project_id,'target_kind',target_kind,'resource_id',resource_id,'version_id',version_id,'transition_id',transition_id,'workflow_version_id',workflow_version_id,'status',status,'revision',revision,'requested_by',requested_by,'created_at',created_at,'expires_at',expires_at,'approval_id',COALESCE(task_approval_id,document_approval_id)) FROM approval_requests WHERE project_id=$1 AND ($2::text IS NULL OR id>$2) AND ($3::text IS NULL OR status=$3) AND ((target_kind='task' AND $4) OR (target_kind='document_version' AND $5)) ORDER BY id LIMIT $6",&[&project,&after,&status,&access.0,&access.1,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<ApprovalRequestSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(ApprovalRequestPage { items, next_after })
    }
}
