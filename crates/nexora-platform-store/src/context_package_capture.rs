use crate::{
    PlatformStore, document_policy_store::DocumentPolicyStore,
    platform_authority::PlatformAuthority, project_context_policy::ProjectContextPolicy,
    task_dependency_lock::TaskDependencyLock, task_read_authority::TaskReadAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use std::collections::BTreeSet;
pub(crate) struct ContextPackageCapture;
impl ContextPackageCapture {
    pub fn capture(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectContextPackageCreate,
    ) -> Result<ProjectContextPackageContent, PlatformError> {
        DocumentPolicyStore::authorize(tx, ctx, project, Capability::DocumentsRead)?;
        let mut task_ids = input.task_ids.clone();
        task_ids.sort_unstable();
        if input.document_versions.len() > 32
            || task_ids.len() > 32
            || task_ids.windows(2).any(|pair| pair[0] == pair[1])
            || (task_ids.is_empty()
                && input.document_versions.is_empty()
                && input.context_manifest_id.is_none())
        {
            return Err(PlatformError::Validation(
                "package needs a bounded nonempty distinct context selection",
            ));
        }
        let mut task_values = Vec::new();
        if !task_ids.is_empty() {
            PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(project))?;
            TaskDependencyLock::shared(tx, project)?;
            for id in task_ids {
                let value = tx
                    .query_opt(
                        "SELECT to_jsonb(t) FROM tasks t WHERE id=$1 AND project_id=$2 FOR SHARE",
                        &[&id, &project],
                    )
                    .map_err(PlatformStore::database_error)?
                    .ok_or(PlatformError::NotFound)?
                    .get(0);
                TaskReadAuthority::require_task(tx, ctx, project, &id)?;
                task_values.push(PlatformStore::decode::<WorkTask>(value)?);
            }
        }
        let mut references = input.document_versions.clone();
        references.sort_unstable_by(|a, b| {
            (&a.document_id, &a.version_id).cmp(&(&b.document_id, &b.version_id))
        });
        if references.windows(2).any(|pair| {
            pair[0].document_id == pair[1].document_id && pair[0].version_id == pair[1].version_id
        }) {
            return Err(PlatformError::Validation(
                "duplicate package document version",
            ));
        }
        let manifest = input
            .context_manifest_id
            .as_deref()
            .map(|id| ProjectContextPolicy::get(tx, project, id))
            .transpose()?;
        let mut manifest_ids = BTreeSet::new();
        if let Some(manifest) = &manifest {
            manifest_ids.insert(manifest.id.clone());
        }
        for reference in &references {
            let version = DocumentPolicyStore::version(
                tx,
                project,
                &reference.document_id,
                &reference.version_id,
            )?;
            if let Some(id) = version.context_manifest_id {
                manifest_ids.insert(id);
            }
        }
        for id in manifest_ids {
            let value = ProjectContextPolicy::get(tx, project, &id)?;
            ProjectContextPolicy::lock_sources(tx, &value)?;
        }
        let document_ids = references
            .iter()
            .map(|reference| &reference.document_id)
            .collect::<BTreeSet<_>>();
        for id in document_ids {
            tx.query_opt(
                "SELECT id FROM project_documents WHERE id=$1 AND project_id=$2 FOR SHARE",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        }
        let mut documents = Vec::new();
        for reference in references {
            let root = DocumentPolicyStore::definition(tx, project, &reference.document_id, false)?;
            let version = DocumentPolicyStore::version(
                tx,
                project,
                &reference.document_id,
                &reference.version_id,
            )?;
            documents.push(PackageDocumentSnapshot {
                document_revision: root.revision,
                published_version_id: root.published_version_id,
                version,
            });
        }
        Ok(ProjectContextPackageContent {
            manifest,
            documents,
            tasks: task_values,
        })
    }
}
