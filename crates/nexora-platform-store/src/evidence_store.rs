use crate::{
    PlatformStore, SecretGenerator, document_policy_store::DocumentPolicyStore,
    evidence_validation::EvidenceValidation, platform_authority::PlatformAuthority,
    repository_access::RepositoryAccess, repository_snapshot_scope::RepositorySnapshotScope,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{
    AuthContext, Capability, EvidenceContent, EvidenceRecord, EvidenceUpload, PlatformError,
};
use serde_json::json;
impl PlatformStore {
    pub fn upload_evidence(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &EvidenceUpload,
    ) -> Result<EvidenceRecord, PlatformError> {
        let bytes = EvidenceValidation::decode(input)?;
        let request = self.request_context.clone();
        let namespace = self.namespace.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::KnowledgeWrite, Some(project))?;
        RepositoryAccess::lock(&mut tx, ctx, project, &input.repository_id)?;
        if let Some(snapshot) = &input.snapshot {
            RepositorySnapshotScope::lock(
                &mut tx,
                ctx,
                project,
                &input.repository_id,
                Some(snapshot),
            )?;
        }
        if let (Some(document), Some(version)) = (&input.document_id, &input.document_version_id) {
            DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
            DocumentPolicyStore::version(&mut tx, project, document, version)?;
        }
        if let Some(value) = crate::receipt_writer::ReceiptWriter::read(
            &mut tx,
            &namespace,
            request.as_ref(),
            ctx.actor_id(),
            "evidence.upload",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let record = EvidenceRecord {
            id: SecretGenerator::id("evd")?,
            project_id: project.into(),
            repository_id: input.repository_id.clone(),
            snapshot: input.snapshot.clone(),
            document_id: input.document_id.clone(),
            document_version_id: input.document_version_id.clone(),
            name: input.name.clone(),
            media_type: input.media_type.clone(),
            phase: input.phase.clone(),
            description: input.description.clone(),
            sha256: SecretGenerator::checksum(&bytes),
            size_bytes: bytes.len() as i64,
            provided_metadata: input.provided_metadata.clone(),
            created_by: ctx.actor_id().into(),
            created_at: tx
                .query_one("SELECT clock_timestamp()::text", &[])
                .map_err(Self::database_error)?
                .get(0),
            trust: "actor_provided_not_independently_verified".into(),
        };
        let value = serde_json::to_value(&record).map_err(|_| PlatformError::Storage)?;
        tx.execute("INSERT INTO evidence_files(id,project_id,repository_id,snapshot_id,document_id,document_version_id,payload,record) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&record.id,&project,&record.repository_id,&record.snapshot,&record.document_id,&record.document_version_id,&bytes,&value]).map_err(Self::database_error)?;
        crate::audit_writer::AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "evidence.uploaded",
            Some(&record.id),
            json!({"project_id":project,"repository_id":record.repository_id,"sha256":record.sha256,"size_bytes":record.size_bytes,"phase":record.phase}),
        )?;
        crate::receipt_writer::ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(record)
    }
    pub fn evidence_content(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<EvidenceContent, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::KnowledgeRead, Some(project))
            .map_err(RepositoryAccess::conceal)?;
        let row=tx.query_opt("SELECT repository_id,record,payload,snapshot_id,document_id,document_version_id FROM evidence_files WHERE project_id=$1 AND id=$2",&[&project,&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        let repository: String = row.get(0);
        RepositoryAccess::lock(&mut tx, ctx, project, &repository)?;
        let record: EvidenceRecord = Self::decode(row.get(1))?;
        if let Some(snapshot) = &record.snapshot {
            RepositorySnapshotScope::lock(&mut tx, ctx, project, &repository, Some(snapshot))?;
        }
        if let (Some(document), Some(version)) = (&record.document_id, &record.document_version_id)
        {
            DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
            DocumentPolicyStore::version(&mut tx, project, document, version)?;
        }
        let bytes: Vec<u8> = row.get(2);
        if record.snapshot != row.get::<_, Option<String>>(3)
            || record.document_id != row.get::<_, Option<String>>(4)
            || record.document_version_id != row.get::<_, Option<String>>(5)
            || record.id != id
            || record.project_id != project
            || record.repository_id != repository
            || record.sha256 != SecretGenerator::checksum(&bytes)
            || record.size_bytes != bytes.len() as i64
        {
            return Err(PlatformError::Storage);
        }
        EvidenceValidation::content(&record.media_type, &bytes)
            .map_err(|_| PlatformError::Storage)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(EvidenceContent {
            record,
            data_base64: STANDARD.encode(bytes),
        })
    }
    pub fn evidence_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        after: Option<&str>,
        limit: u16,
    ) -> Result<serde_json::Value, PlatformError> {
        if limit == 0 || limit > 100 || after.is_some_and(|s| s.len() > 128) {
            return Err(PlatformError::Validation("invalid evidence page"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        RepositoryAccess::lock(&mut tx, ctx, project, repository)?;
        let documents_read = match PlatformAuthority::require(
            &mut tx,
            ctx,
            Capability::DocumentsRead,
            Some(project),
        ) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(error) => return Err(error),
        };
        let rows=tx.query("SELECT id,record,snapshot_id,document_id,document_version_id FROM evidence_files WHERE project_id=$1 AND repository_id=$2 AND ($3::text IS NULL OR id>$3) AND (document_id IS NULL OR $5) ORDER BY id LIMIT $4",&[&project,&repository,&after,&(i64::from(limit)+1),&documents_read]).map_err(Self::database_error)?;
        let more = rows.len() > usize::from(limit);
        let mut records = Vec::new();
        let mut last_scanned = None;
        let mut snapshots = std::collections::BTreeMap::new();
        for row in rows.into_iter().take(usize::from(limit)) {
            let id: String = row.get(0);
            last_scanned = Some(id.clone());
            let record: EvidenceRecord = Self::decode(row.get(1))?;
            if record.id != id
                || record.project_id != project
                || record.repository_id != repository
                || record.snapshot != row.get::<_, Option<String>>(2)
                || record.document_id != row.get::<_, Option<String>>(3)
                || record.document_version_id != row.get::<_, Option<String>>(4)
            {
                return Err(PlatformError::Storage);
            }
            if let Some(snapshot) = &record.snapshot {
                let allowed = if let Some(allowed) = snapshots.get(snapshot) {
                    *allowed
                } else {
                    let allowed = match RepositorySnapshotScope::lock(
                        &mut tx,
                        ctx,
                        project,
                        repository,
                        Some(snapshot),
                    ) {
                        Ok(_) => true,
                        Err(
                            PlatformError::NotFound
                            | PlatformError::Forbidden
                            | PlatformError::Conflict(_),
                        ) => false,
                        Err(error) => return Err(error),
                    };
                    snapshots.insert(snapshot.clone(), allowed);
                    allowed
                };
                if !allowed {
                    continue;
                }
            }
            if let (Some(document), Some(version)) =
                (&record.document_id, &record.document_version_id)
            {
                match DocumentPolicyStore::version(&mut tx, project, document, version) {
                    Ok(_) => {}
                    Err(PlatformError::NotFound) => continue,
                    Err(error) => return Err(error),
                }
            }
            records.push(record);
        }
        let next = if more { last_scanned } else { None };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":records,"next_after":next}))
    }
}
