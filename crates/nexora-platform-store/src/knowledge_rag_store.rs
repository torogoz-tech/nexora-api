use crate::{PlatformStore, SecretGenerator};
use nexora_platform::PlatformError;
use nexora_reasoning::OpenAiClient;
use nexora_retrieval::{KnowledgeChunker, KnowledgeKind};
use postgres::Transaction;
use serde_json::Value;
use std::collections::BTreeMap;

impl PlatformStore {
    pub fn index_code_snapshot_embeddings(
        &mut self,
        project_id: &str,
        repository_id: &str,
        snapshot_id: &str,
        sources: &BTreeMap<String, String>,
        client: &OpenAiClient,
        embedding_model: &str,
    ) -> Result<(), PlatformError> {
        let chunker = KnowledgeChunker::new(1800, 180);
        let chunks = sources
            .iter()
            .flat_map(|(path, content)| {
                chunker
                    .chunk(KnowledgeKind::CodeFact, path, 1, path, content)
                    .into_iter()
                    .map(move |chunk| (path, chunk))
            })
            .collect::<Vec<_>>();
        for batch in chunks.chunks(64) {
            let inputs = batch
                .iter()
                .map(|(_, chunk)| chunk.content.clone())
                .collect::<Vec<_>>();
            let embeddings = client
                .embed_many(embedding_model, &inputs)
                .map_err(|_| PlatformError::Unavailable)?;
            let mut tx = self.client.transaction().map_err(Self::database_error)?;
            for ((path, chunk), embedding) in batch.iter().zip(embeddings) {
                let literal = format!(
                    "[{}]",
                    embedding
                        .iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let dimension = embedding.len() as i32;
                tx.execute(
                    "INSERT INTO code_snapshot_embeddings(project_id,repository_id,snapshot_id,path,sequence,title,content,content_hash,embedding_model,dimension,embedding,published) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::vector,false) ON CONFLICT(project_id,repository_id,snapshot_id,path,sequence,embedding_model,dimension) DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,embedding=EXCLUDED.embedding,published=false,created_at=clock_timestamp()",
                    &[&project_id, &repository_id, &snapshot_id, path, &(chunk.sequence as i32), &chunk.title, &chunk.content, &chunk.content_hash, &embedding_model, &dimension, &literal],
                )
                .map_err(Self::database_error)?;
            }
            tx.commit().map_err(Self::database_error)?;
        }
        Ok(())
    }

    pub(crate) fn enqueue_knowledge_processing(
        tx: &mut Transaction<'_>,
        source_kind: &str,
        resource_id: &str,
        target: &Value,
    ) -> Result<Value, PlatformError> {
        let revision = target["revision"].as_i64().ok_or(PlatformError::Storage)?;
        let project_id = target["project_id"].as_str();
        let serialized = serde_json::to_vec(target).map_err(|_| PlatformError::Storage)?;
        let content_hash = target["checksum"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| SecretGenerator::checksum(&serialized));
        let id = SecretGenerator::id("krag")?;
        let row = tx
            .query_one(
                "INSERT INTO knowledge_processing_jobs(id,project_id,source_kind,resource_id,source_revision,content_hash,status) VALUES($1,$2,$3,$4,$5,$6,'queued') ON CONFLICT(source_kind,resource_id,source_revision,content_hash) DO UPDATE SET status=CASE WHEN knowledge_processing_jobs.status IN ('failed','cancelled') THEN 'queued' ELSE knowledge_processing_jobs.status END,updated_at=clock_timestamp() RETURNING to_jsonb(knowledge_processing_jobs)",
                &[&id, &project_id, &source_kind, &resource_id, &revision, &content_hash],
            )
            .map_err(Self::database_error)?;
        Ok(row.get(0))
    }

    pub(crate) fn cancel_knowledge_processing(
        tx: &mut Transaction<'_>,
        source_kind: &str,
        resource_id: &str,
        revision: i64,
    ) -> Result<(), PlatformError> {
        tx.execute(
            "UPDATE knowledge_processing_jobs SET status='cancelled',lease_owner=NULL,lease_token_digest=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE source_kind=$1 AND resource_id=$2 AND source_revision=$3 AND status IN ('queued','processing','structured')",
            &[&source_kind, &resource_id, &revision],
        )
        .map_err(Self::database_error)?;
        Ok(())
    }

    pub fn knowledge_processing_status(
        &mut self,
        resource_id: &str,
        revision: i64,
    ) -> Result<Value, PlatformError> {
        let row = self
            .client
            .query_opt(
                "SELECT to_jsonb(j) FROM knowledge_processing_jobs j WHERE resource_id=$1 AND source_revision=$2 ORDER BY created_at DESC LIMIT 1",
                &[&resource_id, &revision],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        Ok(row.get(0))
    }

    pub fn claim_knowledge_processing(&mut self) -> Result<Option<Value>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.execute(
            "UPDATE knowledge_processing_jobs SET status='failed',last_error='processing attempts exhausted',updated_at=clock_timestamp() WHERE status='processing' AND lease_until<=clock_timestamp() AND attempts>=3",
            &[],
        )
        .map_err(Self::database_error)?;
        let row = tx
            .query_opt(
                "SELECT id,project_id,source_kind,resource_id,source_revision,content_hash FROM knowledge_processing_jobs j WHERE ((status='queued') OR (status='processing' AND lease_until<=clock_timestamp())) AND attempts<3 AND EXISTS(SELECT 1 FROM knowledge_reviews r WHERE r.resource_id=j.resource_id AND r.resource_revision=j.source_revision AND r.status='approved' AND ((r.kind='contribution' AND j.source_kind IN ('document','technical_debt')) OR r.kind=j.source_kind)) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1",
                &[],
            )
            .map_err(Self::database_error)?;
        let Some(row) = row else {
            tx.commit().map_err(Self::database_error)?;
            return Ok(None);
        };
        let id: String = row.get(0);
        let lease_token = SecretGenerator::token("nkr")?;
        let lease_digest = SecretGenerator::digest(&lease_token);
        tx.execute(
            "UPDATE knowledge_processing_jobs SET status='processing',attempts=attempts+1,claimed_at=clock_timestamp(),lease_owner='knowledge-synthesis-worker',lease_token_digest=$2,lease_until=clock_timestamp()+interval '15 minutes',updated_at=clock_timestamp() WHERE id=$1",
            &[&id, &lease_digest],
        )
        .map_err(Self::database_error)?;
        let value = serde_json::json!({
            "id": id,
            "project_id": row.get::<_, Option<String>>(1),
            "source_kind": row.get::<_, String>(2),
            "resource_id": row.get::<_, String>(3),
            "source_revision": row.get::<_, i64>(4),
            "content_hash": row.get::<_, String>(5),
            "lease_token": lease_token,
        });
        tx.commit().map_err(Self::database_error)?;
        Ok(Some(value))
    }

    pub fn finish_knowledge_processing(
        &mut self,
        job: &Value,
        title: &str,
        body: &str,
        kind: KnowledgeKind,
        structure: &Value,
    ) -> Result<(), PlatformError> {
        let id = job["id"].as_str().ok_or(PlatformError::Storage)?;
        let revision = job["source_revision"]
            .as_i64()
            .ok_or(PlatformError::Storage)?;
        let resource_id = job["resource_id"].as_str().ok_or(PlatformError::Storage)?;
        let source_kind = job["source_kind"].as_str().ok_or(PlatformError::Storage)?;
        let lease_token = job["lease_token"].as_str().ok_or(PlatformError::Storage)?;
        let lease_digest = SecretGenerator::digest(lease_token);
        let project_id = job["project_id"].as_str();
        let chunks =
            KnowledgeChunker::new(1800, 180).chunk(kind, resource_id, revision, title, body);
        if !structure.is_object() || structure["summary"].as_str().is_none() {
            return Err(PlatformError::Validation("invalid knowledge structure"));
        }
        let structure_bytes = serde_json::to_vec(structure).map_err(|_| PlatformError::Storage)?;
        let structure_hash = SecretGenerator::checksum(&structure_bytes);
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let active: Option<String> = tx
            .query_opt(
                "SELECT status FROM knowledge_processing_jobs WHERE id=$1 AND source_kind=$2 AND resource_id=$3 AND source_revision=$4 AND lease_token_digest=$5 AND lease_until>clock_timestamp() FOR UPDATE",
                &[&id, &source_kind, &resource_id, &revision, &lease_digest],
            )
            .map_err(Self::database_error)?
            .map(|row| row.get(0));
        if active.as_deref() != Some("processing") {
            return Err(PlatformError::Conflict(
                "knowledge processing job is no longer active",
            ));
        }
        tx.execute(
            "INSERT INTO knowledge_structures(job_id,source_kind,resource_id,source_revision,schema_version,structure,structure_hash,validation_status) VALUES($1,$2,$3,$4,'1.0',$5,$6,'valid') ON CONFLICT(job_id) DO UPDATE SET structure=$5,structure_hash=$6,validation_status='valid'",
            &[&id, &source_kind, &resource_id, &revision, structure, &structure_hash],
        )
        .map_err(Self::database_error)?;
        for chunk in chunks {
            tx.execute(
                "INSERT INTO knowledge_chunks(id,job_id,project_id,source_kind,resource_id,source_revision,sequence,title,content,content_hash,published) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,false) ON CONFLICT(id) DO NOTHING",
                &[&chunk.id, &id, &project_id, &source_kind, &resource_id, &revision, &(chunk.sequence as i32), &chunk.title, &chunk.content, &chunk.content_hash],
            )
            .map_err(Self::database_error)?;
        }
        tx.execute(
            "UPDATE knowledge_processing_jobs SET status='structured',lease_owner=NULL,lease_token_digest=NULL,lease_until=NULL,updated_at=clock_timestamp(),last_error=NULL WHERE id=$1 AND status='processing' AND lease_token_digest=$2 AND lease_until>clock_timestamp()",
            &[&id, &lease_digest],
        )
        .map_err(Self::database_error)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }

    pub fn fail_knowledge_processing(
        &mut self,
        job: &Value,
        error: &str,
    ) -> Result<(), PlatformError> {
        let id = job["id"].as_str().ok_or(PlatformError::Storage)?;
        let lease_token = job["lease_token"].as_str().ok_or(PlatformError::Storage)?;
        let lease_digest = SecretGenerator::digest(lease_token);
        let safe_error = error.chars().take(500).collect::<String>();
        self.client
            .execute(
                "UPDATE knowledge_processing_jobs SET status='failed',last_error=$2,lease_owner=NULL,lease_token_digest=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE id=$1 AND status IN ('queued','processing','structured') AND lease_token_digest=$3 AND lease_until>clock_timestamp()",
                &[&id, &safe_error, &lease_digest],
            )
            .map_err(Self::database_error)?;
        Ok(())
    }

    pub fn knowledge_processing_source(
        &mut self,
        job: &Value,
    ) -> Result<(String, String, KnowledgeKind), PlatformError> {
        let kind = job["source_kind"].as_str().ok_or(PlatformError::Storage)?;
        let resource_id = job["resource_id"].as_str().ok_or(PlatformError::Storage)?;
        let revision = job["source_revision"]
            .as_i64()
            .ok_or(PlatformError::Storage)?;
        let (title, body, contribution_kind) = match kind {
            "document" | "technical_debt" => self
                .client
                .query_opt(
                    "SELECT title,body FROM knowledge_contributions WHERE id=$1 AND revision=$2",
                    &[&resource_id, &revision],
                )
                .map_err(Self::database_error)?
                .map(|row| (row.get(0), row.get(1), kind.to_owned()))
                .ok_or(PlatformError::NotFound)?,
            "contribution" => self
                .client
                .query_opt(
                    "SELECT title,body,kind FROM knowledge_contributions WHERE id=$1 AND revision=$2",
                    &[&resource_id, &revision],
                )
                .map_err(Self::database_error)?
                .map(|row| (row.get(0), row.get(1), row.get(2)))
                .ok_or(PlatformError::NotFound)?,
            "organization_document" => self
                .client
                .query_opt(
                    "SELECT title,body FROM organization_document_versions WHERE document_id=$1 AND revision=$2",
                    &[&resource_id, &revision],
                )
                .map_err(Self::database_error)?
                .map(|row| (row.get(0), row.get(1), kind.to_owned()))
                .ok_or(PlatformError::NotFound)?,
            "project_document" => self
                .client
                .query_opt(
                    "SELECT title,content::text FROM project_document_versions WHERE document_id=$1 AND number=$2",
                    &[&resource_id, &revision],
                )
                .map_err(Self::database_error)?
                .map(|row| (row.get(0), row.get(1), kind.to_owned()))
                .ok_or(PlatformError::NotFound)?,
            _ => return Err(PlatformError::Validation("unsupported knowledge source")),
        };
        let knowledge_kind = match kind {
            "technical_debt" => KnowledgeKind::TechnicalDebt,
            "document" | "organization_document" | "project_document" => KnowledgeKind::Document,
            "contribution" if contribution_kind == "technical_debt" => KnowledgeKind::TechnicalDebt,
            "contribution" => KnowledgeKind::Document,
            _ => return Err(PlatformError::Validation("unsupported knowledge source")),
        };
        Ok((title, body, knowledge_kind))
    }

    pub fn publish_knowledge_embeddings(
        &mut self,
        job: &Value,
        client: &OpenAiClient,
        embedding_model: &str,
    ) -> Result<(), PlatformError> {
        let job_id = job["id"].as_str().ok_or(PlatformError::Storage)?;
        let lease_token = job["lease_token"].as_str().ok_or(PlatformError::Storage)?;
        let lease_digest = SecretGenerator::digest(lease_token);
        let rows = self
            .client
            .query(
                "SELECT id,content,content_hash FROM knowledge_chunks WHERE job_id=$1 ORDER BY sequence",
                &[&job_id],
            )
            .map_err(Self::database_error)?;
        if rows.is_empty() {
            return Err(PlatformError::Validation(
                "knowledge job produced no chunks",
            ));
        }
        let chunks = rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();
        for batch in chunks.chunks(64) {
            let inputs = batch
                .iter()
                .map(|(_, content, _)| content.clone())
                .collect::<Vec<_>>();
            let embeddings = client
                .embed_many(embedding_model, &inputs)
                .map_err(|_| PlatformError::Unavailable)?;
            let mut tx = self.client.transaction().map_err(Self::database_error)?;
            for ((chunk_id, _, content_hash), embedding) in batch.iter().zip(embeddings) {
                let literal = format!(
                    "[{}]",
                    embedding
                        .iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let dimension = embedding.len() as i32;
                tx.execute(
                    "INSERT INTO knowledge_embeddings(chunk_id,embedding_model,dimension,embedding,content_hash,published) VALUES($1,$2,$3,$4::vector,$5,false) ON CONFLICT(chunk_id,embedding_model,dimension) DO UPDATE SET embedding=EXCLUDED.embedding,content_hash=EXCLUDED.content_hash,published=false,created_at=clock_timestamp()",
                    &[&chunk_id, &embedding_model, &dimension, &literal, &content_hash],
                )
                .map_err(Self::database_error)?;
            }
            tx.commit().map_err(Self::database_error)?;
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let status: String = tx
            .query_one(
                "SELECT status FROM knowledge_processing_jobs WHERE id=$1 AND lease_token_digest=$2 AND lease_until>clock_timestamp() FOR UPDATE",
                &[&job_id, &lease_digest],
            )
            .map_err(Self::database_error)?
            .get(0);
        if status != "structured" {
            return Err(PlatformError::Conflict(
                "knowledge processing job is no longer ready for publication",
            ));
        }
        tx.execute(
            "UPDATE knowledge_embeddings e SET published=true,created_at=clock_timestamp() FROM knowledge_chunks c WHERE c.id=e.chunk_id AND c.job_id=$1 AND c.content_hash=e.content_hash AND e.embedding_model=$2",
            &[&job_id, &embedding_model],
        )
        .map_err(Self::database_error)?;
        tx.execute(
            "UPDATE knowledge_chunks SET published=true WHERE job_id=$1",
            &[&job_id],
        )
        .map_err(Self::database_error)?;
        tx.execute(
            "UPDATE knowledge_processing_jobs SET status='indexed',lease_owner=NULL,lease_token_digest=NULL,lease_until=NULL,updated_at=clock_timestamp(),last_error=NULL WHERE id=$1 AND status='structured' AND lease_token_digest=$2 AND lease_until>clock_timestamp()",
            &[&job_id, &lease_digest],
        )
        .map_err(Self::database_error)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }
}
