use crate::{
    PlatformStore, document_verified_context::DocumentVerifiedContext,
    project_context_checksum::ProjectContextChecksum, project_context_policy::ProjectContextPolicy,
    repository_snapshot_scope::RepositorySnapshotScope,
};
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::*;
use nexora_runtime::{DatabaseRuntime, KnowledgeAnchor, KnowledgeQuery};
use postgres::Transaction;
use serde_json::json;
use std::collections::BTreeSet;
pub(crate) struct DocumentContextStore;
impl DocumentContextStore {
    pub fn capture(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        manifest_id: Option<&str>,
        inputs: &[DocumentEvidenceInput],
        mut runtime: Option<&mut DatabaseRuntime>,
    ) -> Result<DocumentVerifiedContext, PlatformError> {
        if inputs.len() > 8 {
            return Err(PlatformError::Validation(
                "document evidence exceeds8 anchors",
            ));
        }
        let Some(manifest_id) = manifest_id else {
            if !inputs.is_empty() {
                return Err(PlatformError::Validation(
                    "evidence requires a context manifest",
                ));
            }
            return Ok(DocumentVerifiedContext::default());
        };
        let manifest = ProjectContextPolicy::get(tx, project, manifest_id)?;
        let mut scopes = Vec::new();
        for member in &manifest.members {
            let scope = RepositorySnapshotScope::lock(
                tx,
                ctx,
                project,
                &member.repository_id,
                Some(&member.snapshot_id),
            )?;
            let (connection_id, connection_revision) = scope.connection(tx)?;
            if scope.source_generation != member.source_generation
                || connection_id != member.connection_id
                || connection_revision != member.connection_revision
                || scope.commit != member.pinned_commit
            {
                return Err(PlatformError::NotFound);
            }
            match (&scope.local_origin, &member.local_origin) {
                (Some(current), Some(recorded))
                    if current.source_id == recorded.source_id
                        && current.authorization_revision == recorded.authorization_revision =>
                {
                    tx.query_opt("SELECT id FROM local_snapshot_publications WHERE source_id=$1 AND id=$2 AND authorization_revision=$3 AND snapshot_id=$4 AND payload_hash=$5 FOR SHARE", &[&recorded.source_id,&recorded.publication_id,&recorded.authorization_revision,&member.snapshot_id,&member.payload_hash])
                        .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
                }
                (None, None) => {
                    // Historical scopes already proved a successful job. Preserve the
                    // evidence-specific job check for the current Git binding as well.
                    if scope.expected_hash.is_some() {
                        tx.query_opt("SELECT id FROM repository_jobs WHERE project_id=$1 AND repository_id=$2 AND snapshot_id=$3 AND state='succeeded' AND source_generation=$4 AND connection_id=$5 AND connection_revision=$6 AND pinned_commit=$7 LIMIT 1", &[&project,&member.repository_id,&member.snapshot_id,&member.source_generation,&member.connection_id,&member.connection_revision,&member.pinned_commit])
                            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
                    }
                }
                _ => return Err(PlatformError::NotFound),
            }
            scopes.push(scope);
        }
        let mut evidence = Vec::new();
        let mut seen = BTreeSet::new();
        for input in inputs {
            let anchor = KnowledgeAnchor {
                path: input.path.clone(),
                start_line: input.start_line,
                line_count: input.line_count,
            };
            anchor
                .validate()
                .map_err(|_| PlatformError::Validation("invalid document source anchor"))?;
            if !seen.insert((
                &input.repository_id,
                &input.path,
                input.start_line,
                input.line_count,
            )) {
                return Err(PlatformError::Validation(
                    "duplicate document evidence anchor",
                ));
            }
            let index = manifest
                .members
                .iter()
                .position(|member| member.repository_id == input.repository_id)
                .ok_or(PlatformError::Validation(
                    "evidence repository is outside the manifest",
                ))?;
            let member = &manifest.members[index];
            let runtime = runtime.as_deref_mut().ok_or(PlatformError::Storage)?;
            let tenant = TenantId(scopes[index].runtime_tenant_id.clone());
            let repository = RepositoryId(scopes[index].runtime_repository_id.clone());
            let snapshot = SnapshotId(member.snapshot_id.clone());
            let head = runtime
                .query_header(&tenant, &repository, Some(&snapshot))
                .map_err(|_| PlatformError::Storage)?;
            if head.payload_hash != member.payload_hash {
                return Err(PlatformError::Conflict(
                    "evidence snapshot checksum changed",
                ));
            }
            let value = runtime
                .query(
                    &tenant,
                    &repository,
                    Some(&snapshot),
                    &KnowledgeQuery::Context {
                        path: input.path.clone(),
                        start_line: input.start_line,
                        line_count: input.line_count,
                    },
                )
                .map_err(|_| PlatformError::Storage)?;
            let items = value["items"].as_array().ok_or(PlatformError::Storage)?;
            if items.len() != input.line_count
                || items.iter().enumerate().any(|(offset, item)| {
                    item["text_truncated"] != false
                        || item["line"].as_u64() != Some((input.start_line + offset) as u64)
                        || item["path"].as_str() != Some(input.path.as_str())
                        || !item["text"].is_string()
                })
            {
                return Err(PlatformError::Validation(
                    "document evidence range is missing or truncated",
                ));
            }
            let checksum = ProjectContextChecksum::value(
                json!({"project_id":project,"manifest_id":manifest.id,"repository_id":member.repository_id,"snapshot_id":member.snapshot_id,"payload_hash":member.payload_hash,"path":input.path,"start_line":input.start_line,"line_count":input.line_count,"items":items}),
            )?;
            evidence.push(DocumentEvidence {
                repository_id: member.repository_id.clone(),
                snapshot_id: member.snapshot_id.clone(),
                pinned_commit: member.pinned_commit.clone(),
                payload_hash: member.payload_hash.clone(),
                path: input.path.clone(),
                start_line: input.start_line,
                line_count: input.line_count,
                context_checksum: checksum,
                local_origin: member.local_origin.clone(),
            });
        }
        evidence.sort_unstable_by(|a, b| {
            (&a.repository_id, &a.path, a.start_line, a.line_count).cmp(&(
                &b.repository_id,
                &b.path,
                b.start_line,
                b.line_count,
            ))
        });
        Ok(DocumentVerifiedContext {
            manifest_id: Some(manifest.id),
            evidence,
        })
    }
    pub fn lock_version(
        tx: &mut Transaction<'_>,
        version: &ProjectDocumentVersion,
    ) -> Result<(), PlatformError> {
        if let Some(id) = version.context_manifest_id.as_deref() {
            let manifest = ProjectContextPolicy::get(tx, &version.project_id, id)?;
            ProjectContextPolicy::lock_sources(tx, &manifest)?;
        }
        Ok(())
    }
}
