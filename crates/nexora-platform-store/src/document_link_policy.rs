use crate::{
    PlatformStore, document_link_scope::DocumentLinkScope,
    document_policy_store::DocumentPolicyStore, platform_authority::PlatformAuthority,
    task_policy_store::TaskPolicyStore, task_read_authority::TaskReadAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct DocumentLinkPolicy;
impl DocumentLinkPolicy {
    pub fn authority(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        target: &DocumentLinkTarget,
    ) -> Result<(), PlatformError> {
        DocumentPolicyStore::authorize(tx, ctx, project, Capability::DocumentsWrite)?;
        DocumentPolicyStore::authorize(tx, ctx, project, Capability::DocumentsRead)?;
        if let DocumentLinkTarget::Task { task_id, .. } = target {
            for capability in [Capability::TasksRead, Capability::TasksWrite] {
                PlatformAuthority::require(tx, ctx, capability, Some(project))?;
            }
            // Lock before checking individual visibility, including receipt replays.
            // Reassignment cannot revoke visibility between this check and the write.
            TaskPolicyStore::task(tx, project, task_id)?;
            TaskReadAuthority::require_task(tx, ctx, project, task_id)?;
        }
        Ok(())
    }
    pub fn lock(
        tx: &mut Transaction<'_>,
        project: &str,
        source: &str,
        target: &DocumentLinkTarget,
    ) -> Result<DocumentLinkScope, PlatformError> {
        let mut ids = BTreeSet::from([source.to_string()]);
        let mut task = None;
        match target {
            DocumentLinkTarget::Task {
                task_id,
                expected_revision,
            } => {
                let value = TaskPolicyStore::task(tx, project, task_id)?;
                if value.revision != *expected_revision {
                    return Err(PlatformError::Conflict("target task revision changed"));
                }
                task = Some(value);
            }
            DocumentLinkTarget::DocumentVersion { document_id, .. } => {
                ids.insert(document_id.clone());
            }
        }
        let mut documents = BTreeMap::new();
        for id in ids {
            documents.insert(
                id.clone(),
                DocumentPolicyStore::definition(tx, project, &id, true)?,
            );
        }
        let source = documents
            .get(source)
            .ok_or(PlatformError::NotFound)?
            .clone();
        let target_document = if let DocumentLinkTarget::DocumentVersion {
            document_id,
            expected_revision,
            ..
        } = target
        {
            let value = documents
                .get(document_id)
                .ok_or(PlatformError::NotFound)?
                .clone();
            if value.revision != *expected_revision {
                return Err(PlatformError::Conflict("target document revision changed"));
            }
            Some(value)
        } else {
            None
        };
        Ok(DocumentLinkScope {
            source,
            target_document,
            task,
        })
    }
    pub fn get(
        tx: &mut Transaction<'_>,
        project: &str,
        document: &str,
        id: &str,
    ) -> Result<DocumentLink, PlatformError> {
        let value=tx.query_opt("SELECT to_jsonb(l) FROM document_links l WHERE id=$1 AND project_id=$2 AND source_document_id=$3",&[&id,&project,&document]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        PlatformStore::decode(value)
    }
    pub fn target(link: &DocumentLink, revision: i64) -> Result<DocumentLinkTarget, PlatformError> {
        if let Some(task_id) = &link.target_task_id {
            return Ok(DocumentLinkTarget::Task {
                task_id: task_id.clone(),
                expected_revision: revision,
            });
        }
        Ok(DocumentLinkTarget::DocumentVersion {
            document_id: link
                .target_document_id
                .clone()
                .ok_or(PlatformError::Storage)?,
            version_id: link
                .target_version_id
                .clone()
                .ok_or(PlatformError::Storage)?,
            expected_revision: revision,
        })
    }
}
