use crate::{
    PlatformStore, document_policy_store::DocumentPolicyStore,
    platform_authority::PlatformAuthority, project_context_checksum::ProjectContextChecksum,
    project_context_policy::ProjectContextPolicy, task_dependency_lock::TaskDependencyLock,
    task_read_authority::TaskReadAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::json;
use std::collections::BTreeSet;
pub(crate) struct ContextPackagePolicy;
impl ContextPackagePolicy {
    pub fn checksum(value: &ProjectContextPackage) -> Result<String, PlatformError> {
        ProjectContextChecksum::value(
            json!({"id":value.id,"project_id":value.project_id,"name":value.name,"content":value.content,"reason":value.reason,"created_by":value.created_by}),
        )
    }
    pub fn get(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectContextPackage, PlatformError> {
        DocumentPolicyStore::authorize(tx, ctx, project, Capability::DocumentsRead)?;
        let value = tx
            .query_opt(
                "SELECT to_jsonb(p) FROM project_context_packages p WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let mut result: ProjectContextPackage = PlatformStore::decode(value)?;
        if !result.content.tasks.is_empty() {
            PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(project)).map_err(
                |error| {
                    if matches!(error, PlatformError::Forbidden) {
                        PlatformError::NotFound
                    } else {
                        error
                    }
                },
            )?;
        }
        if Self::checksum(&result)? != result.checksum {
            return Err(PlatformError::Conflict("context package checksum changed"));
        }
        // Check every captured task before freshness inspection, which may return
        // early. Historical snapshots never preserve a reader's former access.
        if !result.content.tasks.is_empty() {
            TaskDependencyLock::shared(tx, project)?;
            let mut tasks = result.content.tasks.iter().collect::<Vec<_>>();
            tasks.sort_unstable_by(|a, b| a.id.cmp(&b.id));
            for task in tasks {
                if task.project_id.as_deref() != Some(project) {
                    return Err(PlatformError::Storage);
                }
                tx.query_opt(
                    "SELECT id FROM tasks WHERE id=$1 AND project_id=$2 FOR SHARE",
                    &[&task.id, &project],
                )
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::NotFound)?;
                TaskReadAuthority::require_task(tx, ctx, project, &task.id)?;
            }
        }
        result.needs_review = Self::needs_review(tx, &result)?;
        Ok(result)
    }
    fn needs_review(
        tx: &mut Transaction<'_>,
        package: &ProjectContextPackage,
    ) -> Result<bool, PlatformError> {
        let project = &package.project_id;
        let mut manifests = BTreeSet::new();
        if let Some(manifest) = &package.content.manifest {
            manifests.insert(manifest.id.clone());
        }
        for captured in &package.content.documents {
            if captured.version.project_id != *project {
                return Err(PlatformError::Storage);
            }
            let root =
                DocumentPolicyStore::definition(tx, project, &captured.version.document_id, false)?;
            let current = DocumentPolicyStore::version(
                tx,
                project,
                &captured.version.document_id,
                &captured.version.id,
            )?;
            if root.revision != captured.document_revision
                || root.published_version_id != captured.published_version_id
                || current.state_revision != captured.version.state_revision
                || current.checksum != captured.version.checksum
                || current.needs_review
            {
                return Ok(true);
            }
            if let Some(id) = &captured.version.context_manifest_id {
                manifests.insert(id.clone());
            }
        }
        for task in &package.content.tasks {
            if task.project_id.as_deref() != Some(project.as_str()) {
                return Err(PlatformError::Storage);
            }
            let row = tx
                .query_opt(
                    "SELECT revision FROM tasks WHERE id=$1 AND project_id=$2",
                    &[&task.id, &project],
                )
                .map_err(PlatformStore::database_error)?;
            if row.is_none_or(|row| row.get::<_, i64>(0) != task.revision) {
                return Ok(true);
            }
        }
        for id in manifests {
            let manifest = ProjectContextPolicy::get(tx, project, &id)?;
            if ProjectContextPolicy::status(tx, &manifest)?.needs_review {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
