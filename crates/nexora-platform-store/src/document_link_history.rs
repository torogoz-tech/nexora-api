use crate::{
    PlatformStore, document_history_writer::DocumentHistoryWriter,
    document_link_scope::DocumentLinkScope, task_history_writer::TaskHistoryWriter,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::json;
pub(crate) struct DocumentLinkHistory;
impl DocumentLinkHistory {
    #[allow(clippy::too_many_arguments)]
    pub fn touch(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        scope: &DocumentLinkScope,
        version: &str,
        target_version: Option<&str>,
        link_id: &str,
        kind: DocumentLinkKind,
        removed: bool,
        reason: &str,
    ) -> Result<(i64, i64), PlatformError> {
        let project = &scope.source.project_id;
        let source = &scope.source.id;
        let source_revision:i64=tx.query_one("UPDATE project_documents SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING revision",&[&source,&project]).map_err(PlatformStore::database_error)?.get(0);
        let target_revision = if let Some(task) = &scope.task {
            let value=tx.query_one("UPDATE tasks AS t SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(t)",&[&task.id,&project]).map_err(PlatformStore::database_error)?.get(0);
            let updated: WorkTask = PlatformStore::decode(value)?;
            TaskHistoryWriter::event(
                tx,
                ctx,
                &updated,
                if removed {
                    "task.document_unlinked"
                } else {
                    "task.document_linked"
                },
                json!({"link_id":link_id,"document_id":source,"version_id":version,"kind":kind,"reason":reason}),
            )?;
            updated.revision
        } else if let Some(target) = &scope.target_document {
            if target.id == *source {
                source_revision
            } else {
                tx.query_one("UPDATE project_documents SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING revision",&[&target.id,&project]).map_err(PlatformStore::database_error)?.get(0)
            }
        } else {
            return Err(PlatformError::Storage);
        };
        let details = json!({"link_id":link_id,"kind":kind,"target_task_id":scope.task.as_ref().map(|task|&task.id),"target_document_id":scope.target_document.as_ref().map(|doc|&doc.id),"target_version_id":target_version,"source_revision":source_revision,"target_revision":target_revision});
        DocumentHistoryWriter::append(
            tx,
            ctx,
            project,
            source,
            version,
            if removed {
                "document.link_removed"
            } else {
                "document.link_created"
            },
            reason,
            details,
        )?;
        if let (Some(target), Some(target_version)) = (&scope.target_document, target_version) {
            DocumentHistoryWriter::append(
                tx,
                ctx,
                project,
                &target.id,
                target_version,
                if removed {
                    "document.incoming_link_removed"
                } else {
                    "document.incoming_link_created"
                },
                reason,
                json!({"link_id":link_id,"source_document_id":source,"source_version_id":version,"kind":kind}),
            )?;
        }
        Ok((source_revision, target_revision))
    }
}
