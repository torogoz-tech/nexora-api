use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    knowledge_contribution_policy::KnowledgeContributionPolicy, knowledge_file::KnowledgeFile,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, DomainText, KnowledgeContributionInput, PlatformError};
use serde_json::{Value, json};
pub(crate) struct KnowledgeContributionStore;
impl KnowledgeContributionStore {
    pub fn write(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: Option<&str>,
        input: &KnowledgeContributionInput,
        bytes: Option<&[u8]>,
    ) -> Result<Value, PlatformError> {
        let title = DomainText::required(&input.title, 256)?;
        if !["document", "technical_debt"].contains(&input.kind.as_str()) {
            return Err(PlatformError::Validation("invalid contribution kind"));
        }
        let body = if bytes.is_none() || input.kind == "technical_debt" {
            DomainText::required(&input.body, 131072)?
        } else {
            String::new()
        };
        if id.is_none() && input.expected_revision.is_some() {
            return Err(PlatformError::Validation(
                "new contribution has no revision",
            ));
        }
        let mime = if input.kind == "document" && bytes.is_some() {
            Some(KnowledgeFile::mime(
                input.filename.as_deref().unwrap_or(""),
                bytes.unwrap_or_default(),
            )?)
        } else {
            None
        };
        if mime.is_none() && (bytes.is_some() || input.filename.is_some()) {
            return Err(PlatformError::Validation(
                "files belong only to new document contributions",
            ));
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        KnowledgeContributionPolicy::write(
            &mut tx,
            ctx,
            input.project_id.as_deref(),
            id.is_some() && bytes.is_none(),
        )?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "knowledge.contribute",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(v);
        }
        let key = match id {
            Some(id) => id.to_owned(),
            None => SecretGenerator::id("knc")?,
        };
        if id.is_some() {
            let row=tx.query_opt("SELECT project_id,kind,revision,status FROM knowledge_contributions WHERE id=$1 FOR UPDATE",&[&key]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
            if row.get::<_, Option<String>>(0) != input.project_id
                || row.get::<_, String>(1) != input.kind
            {
                return Err(PlatformError::Validation(
                    "contribution scope and kind cannot change",
                ));
            }
            if Some(row.get::<_, i64>(2)) != input.expected_revision {
                return Err(PlatformError::Conflict("contribution changed"));
            }
            if !["ready", "failed"].contains(&row.get::<_, String>(3).as_str()) {
                return Err(PlatformError::Conflict("wait for document processing"));
            }
            if let Some(mime) = mime {
                let sha = bytes.map(SecretGenerator::checksum);
                tx.execute("UPDATE knowledge_contributions SET title=$2,body='',filename=$3,mime_type=$4,payload=$5,sha256=$6,source_revision=source_revision+1,revision=revision+1,status='queued',updated_at=now() WHERE id=$1",&[&key,&title,&input.filename,&mime,&bytes,&sha]).map_err(PlatformStore::database_error)?;
            } else {
                tx.execute("UPDATE knowledge_contributions SET title=$2,body=$3,status='ready',revision=revision+1,updated_at=now() WHERE id=$1",&[&key,&title,&body]).map_err(PlatformStore::database_error)?;
            }
        } else {
            let sha = bytes.map(SecretGenerator::checksum);
            let status = if mime.is_some() { "queued" } else { "ready" };
            tx.execute("INSERT INTO knowledge_contributions(id,project_id,kind,title,body,filename,mime_type,payload,sha256,status,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",&[&key,&input.project_id,&input.kind,&title,&body,&input.filename,&mime,&bytes,&sha,&status,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        }
        if mime.is_some() {
            tx.execute("INSERT INTO knowledge_contribution_sources(contribution_id,source_revision,filename,mime_type,payload,sha256,created_by) SELECT id,source_revision,filename,mime_type,payload,sha256,$2 FROM knowledge_contributions WHERE id=$1",&[&key,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        }
        tx.execute("INSERT INTO knowledge_contribution_versions(contribution_id,revision,source_revision,title,body,created_by) SELECT id,revision,source_revision,title,body,$2 FROM knowledge_contributions WHERE id=$1",&[&key,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        let value: Value = tx
            .query_one(
                "SELECT to_jsonb(c)-'payload' FROM knowledge_contributions c WHERE id=$1",
                &[&key],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "knowledge.contribute",
            Some(&key),
            json!({"kind":input.kind,"revision":value["revision"],"sha256":value["sha256"]}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
}
